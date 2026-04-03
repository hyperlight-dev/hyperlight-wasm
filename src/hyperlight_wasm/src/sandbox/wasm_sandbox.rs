/*
Copyright 2024 The Hyperlight Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

use std::path::Path;
use std::sync::Arc;

#[cfg(target_os = "linux")]
use hyperlight_host::mem::memory_region::{MemoryRegion, MemoryRegionFlags, MemoryRegionType};
use hyperlight_host::sandbox::snapshot::Snapshot;
use hyperlight_host::{MultiUseSandbox, Result, new_error};

use super::loaded_wasm_sandbox::LoadedWasmSandbox;
use crate::sandbox::metrics::{
    METRIC_ACTIVE_WASM_SANDBOXES, METRIC_SANDBOX_LOADS, METRIC_TOTAL_WASM_SANDBOXES,
};

// All the logic around when to restore is nicely encapsulated here,
// so that it would be harder for a `WasmSandbox` to end up in an
// un-restored state.
mod backing_sandbox {
    use super::*;
    #[derive(Debug)]
    pub(super) enum BackingSandbox {
        /// A sandbox which has a clean copy of the runtime in it
        Clean(MultiUseSandbox),
        /// A sandbox which has had a wasm component/module loaded into
        /// it, but has not yet run any code from that
        Loaded(MultiUseSandbox),
        /// A sandbox which came from a `LoadedWasmSandbox`, and
        /// therefore presumably has run user code
        Dirty(MultiUseSandbox),
        /// A non-existent sandbox, used as an internal implementation
        /// detail of a few methods.
        Missing,
    }
    impl BackingSandbox {
        pub(super) fn clean(&mut self, snapshot: Arc<Snapshot>) -> Result<()> {
            *self = match std::mem::replace(self, BackingSandbox::Missing) {
                BackingSandbox::Clean(x) => BackingSandbox::Clean(x),
                BackingSandbox::Loaded(_) => {
                    return Err(new_error!(
                        "internal invariant violation: cleaning loaded backing sandbox"
                    ));
                }
                BackingSandbox::Dirty(mut x) => {
                    x.restore(snapshot)?;
                    BackingSandbox::Clean(x)
                }
                BackingSandbox::Missing => {
                    return Err(new_error!(
                        "internal invariant violation: cleaning missing backing sandbox"
                    ));
                }
            };
            Ok(())
        }
        pub(super) fn load_via_restore(&mut self, snapshot: Arc<Snapshot>) -> Result<()> {
            *self = match std::mem::replace(self, BackingSandbox::Missing) {
                BackingSandbox::Clean(mut x) | BackingSandbox::Dirty(mut x) => {
                    x.restore(snapshot)?;
                    BackingSandbox::Loaded(x)
                }
                BackingSandbox::Loaded(_) => {
                    return Err(new_error!(
                        "internal invariant violation: loading loaded backing sandbox"
                    ));
                }
                BackingSandbox::Missing => {
                    return Err(new_error!(
                        "internal invariant violation: loading missing backing sandbox"
                    ));
                }
            };
            Ok(())
        }
        pub(super) fn load_via_fn(&mut self, load: impl FnOnce(&mut MultiUseSandbox) -> Result<()>) -> Result<()> {
            *self = match std::mem::replace(self, BackingSandbox::Missing) {
                BackingSandbox::Clean(mut x) => {
                    load(&mut x)?;
                    BackingSandbox::Loaded(x)
                }
                _ => {
                    return Err(new_error!(
                        "internal invariant violation: loading non-clean backing sandbox"
                    ));
                }
            };
            Ok(())
        }
        pub(super) fn get_loaded(&mut self) -> Result<MultiUseSandbox> {
            match std::mem::replace(self, BackingSandbox::Missing) {
                BackingSandbox::Loaded(x) => Ok(x),
                _ => Err(new_error!(
                    "internal invariant violation: encountered non-loaded backing sandbox"
                )),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{*, super::tests::*};
        #[test]
        fn test_backing_sandbox_use_marks_dirty() -> Result<()> {
            let mut sb = SandboxBuilder::new().build()?;
            sb.register(
                "GetTimeSinceBootMicrosecond",
                get_time_since_boot_microsecond,
            )?;
            let sb = sb.load_runtime()?;
            let lb = sb.load_module(get_test_file_path("RunWasm.aot")?)?;
            let sb = lb.unload_module()?;
            assert!(matches!(sb.inner, super::BackingSandbox::Dirty(_)));
            Ok(())
        }

        #[test]
        fn test_dirty_backing_sandbox_cannot_be_loaded_via_fn() -> Result<()> {
            let mut sb = SandboxBuilder::new().build()?;
            sb.register(
                "GetTimeSinceBootMicrosecond",
                get_time_since_boot_microsecond,
            )?;
            let sb = sb.load_runtime()?;
            let lb = sb.load_module(get_test_file_path("RunWasm.aot")?)?;
            let mut sb = lb.unload_module()?;
            assert!(sb.inner.load_via_fn(|_| Ok(())).is_err());
            Ok(())
        }

        #[test]
        fn test_dirty_backing_sandbox_cannot_be_gotten_as_loaded() -> Result<()> {
            let mut sb = SandboxBuilder::new().build()?;
            sb.register(
                "GetTimeSinceBootMicrosecond",
                get_time_since_boot_microsecond,
            )?;
            let sb = sb.load_runtime()?;
            let lb = sb.load_module(get_test_file_path("RunWasm.aot")?)?;
            let mut sb = lb.unload_module()?;
            assert!(sb.inner.get_loaded().is_err());
            Ok(())
        }
    }
}
use backing_sandbox::*;

/// A sandbox with just the Wasm engine loaded into memory. `WasmSandbox`es
/// are not yet ready to execute guest functions.
///
/// Before you can call guest functions, you must call the `load_module`
/// function to load a Wasm module into memory. That function will return a
/// `LoadedWasmSandbox` able to execute code in the loaded Wasm Module.
pub struct WasmSandbox {
    // inner is an Option<MultiUseSandbox> as we need to take ownership of it
    // We implement drop on the WasmSandbox to decrement the count of Sandboxes when it is dropped
    // because of this we cannot implement drop without making inner an Option (alternatively we could make MultiUseSandbox Copy but that would introduce other issues)
    inner: BackingSandbox,
    // Snapshot of state of an initial WasmSandbox (runtime loaded, but no guest module code loaded).
    // Used for LoadedWasmSandbox to be able restore state back to WasmSandbox
    snapshot: Option<Arc<Snapshot>>,
}

const MAPPED_BINARY_VA: u64 = 0x1_0000_0000u64;
impl WasmSandbox {
    /// Create a new WasmSandBox from a `MultiUseSandbox`.
    /// This function should be used to create a new `WasmSandbox` from a ProtoWasmSandbox.
    /// The difference between this function and creating  a `WasmSandbox` directly is that
    /// this function will increment the metrics for the number of `WasmSandbox`es in the system.
    pub(super) fn new(mut inner: MultiUseSandbox) -> Result<Self> {
        let snapshot = inner.snapshot()?;
        metrics::gauge!(METRIC_ACTIVE_WASM_SANDBOXES).increment(1);
        metrics::counter!(METRIC_TOTAL_WASM_SANDBOXES).increment(1);
        Ok(WasmSandbox {
            inner: BackingSandbox::Clean(inner),
            snapshot: Some(snapshot),
        })
    }

    /// Same as new, but doesn't take a new snapshot. Useful if `new` has already been called,
    /// for example when creating a `WasmSandbox` from a `LoadedWasmSandbox`, since
    /// the snapshot has already been created in that case.
    /// Expects a snapshot of the state where wasm runtime is loaded, but no guest module code is loaded.
    pub(super) fn new_from_loaded(
        loaded: MultiUseSandbox,
        snapshot: Arc<Snapshot>,
    ) -> Result<Self> {
        metrics::gauge!(METRIC_ACTIVE_WASM_SANDBOXES).increment(1);
        metrics::counter!(METRIC_TOTAL_WASM_SANDBOXES).increment(1);
        Ok(WasmSandbox {
            inner: BackingSandbox::Dirty(loaded),
            snapshot: Some(snapshot),
        })
    }

    fn clean_inner(&mut self) -> Result<()> {
        let snapshot = self.snapshot.as_ref().ok_or(new_error!(
            "internal invariant violation: Snapshot is missing"
        ))?;
        self.inner.clean(snapshot.clone())
    }

    /// Load a Wasm module at the given path into the sandbox and return a `LoadedWasmSandbox`
    /// able to execute code in the loaded Wasm Module.
    ///
    /// Before you can call guest functions in the sandbox, you must call
    /// this function and use the returned value to call guest functions.
    pub fn load_module(mut self, file: impl AsRef<Path>) -> Result<LoadedWasmSandbox> {
        self.clean_inner()?;

        self.inner.load_via_fn(|inner| {
            if let Ok(len) = inner.map_file_cow(file.as_ref(), MAPPED_BINARY_VA, None) {
                inner.call::<()>("LoadWasmModulePhys", (MAPPED_BINARY_VA, len))?;
            } else {
                let wasm_bytes = std::fs::read(file)?;
                load_wasm_module_from_bytes(inner, wasm_bytes)?;
            }
            Ok(())
        })?;

        self.finalize_module_load()
    }

    /// Load a Wasm module by restoring a Hyperlight snapshot taken
    /// from a `LoadedWasmSandbox`.
    pub fn load_from_snapshot(mut self, snapshot: Arc<Snapshot>) -> Result<LoadedWasmSandbox> {
        self.inner.load_via_restore(snapshot)?;

        self.finalize_module_load()
    }

    /// Load a Wasm module that is currently present in a buffer in
    /// host memory, by mapping the host memory directly into the
    /// sandbox.
    ///
    /// Depending on the host platform, there are likely alignment
    /// requirements of at least one page for base and len
    ///
    /// # Safety
    /// It is the caller's responsibility to ensure that the host side
    /// of the region remains intact and is not written to until the
    /// produced LoadedWasmSandbox is discarded or devolved.
    #[cfg(target_os = "linux")]
    pub unsafe fn load_module_by_mapping(
        mut self,
        base: *mut libc::c_void,
        len: usize,
    ) -> Result<LoadedWasmSandbox> {
        self.clean_inner()?;

        self.inner.load_via_fn(|inner| {
            let guest_base: usize = MAPPED_BINARY_VA as usize;
            let rgn = MemoryRegion {
                host_region: base as usize..base.wrapping_add(len) as usize,
                guest_region: guest_base..guest_base + len,
                flags: MemoryRegionFlags::READ | MemoryRegionFlags::EXECUTE,
                region_type: MemoryRegionType::Heap,
            };
            if let Ok(()) = unsafe { inner.map_region(&rgn) } {
                inner.call::<()>("LoadWasmModulePhys", (MAPPED_BINARY_VA, len as u64))?;
            } else {
                let wasm_bytes =
                    unsafe { std::slice::from_raw_parts(base as *const u8, len).to_vec() };
                load_wasm_module_from_bytes(inner, wasm_bytes)?;
            }
            Ok(())
        })?;

        self.finalize_module_load()
    }

    /// Load a Wasm module from a buffer of bytes into the sandbox and return a `LoadedWasmSandbox`
    /// able to execute code in the loaded Wasm Module.
    ///
    /// Before you can call guest functions in the sandbox, you must call
    /// this function and use the returned value to call guest functions.
    pub fn load_module_from_buffer(mut self, buffer: &[u8]) -> Result<LoadedWasmSandbox> {
        self.clean_inner()?;

        // TODO: get rid of this clone
        self.inner
            .load_via_fn(|inner| load_wasm_module_from_bytes(inner, buffer.to_vec()))?;

        self.finalize_module_load()
    }

    /// Helper function to finalize module loading and create LoadedWasmSandbox
    fn finalize_module_load(mut self) -> Result<LoadedWasmSandbox> {
        metrics::counter!(METRIC_SANDBOX_LOADS).increment(1);

        let sandbox = self.inner.get_loaded()?;

        let snapshot = self.snapshot.take().ok_or(new_error!(
            "internal invariant violation: Snapshot is missing"
        ))?;

        LoadedWasmSandbox::new(sandbox, snapshot)
    }
}

fn load_wasm_module_from_bytes(inner: &mut MultiUseSandbox, wasm_bytes: Vec<u8>) -> Result<()> {
    let res: i32 = inner.call(
        "LoadWasmModule",
        (wasm_bytes.clone(), wasm_bytes.len() as i32),
    )?;
    if res != 0 {
        return Err(new_error!(
            "LoadWasmModule Failed with error code {:?}",
            res
        ));
    }
    Ok(())
}

impl std::fmt::Debug for WasmSandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmSandbox").finish()
    }
}

impl Drop for WasmSandbox {
    fn drop(&mut self) {
        metrics::gauge!(METRIC_ACTIVE_WASM_SANDBOXES).decrement(1);
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::Path;

    use hyperlight_host::{HyperlightError, is_hypervisor_present};

    use super::*;
    pub(super) use crate::sandbox::sandbox_builder::SandboxBuilder;

    #[test]
    fn test_new_sandbox() -> Result<()> {
        let _sandbox = SandboxBuilder::new().build()?;
        Ok(())
    }

    pub(super) fn get_time_since_boot_microsecond() -> Result<i64> {
        let res = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)?
            .as_micros();
        i64::try_from(res).map_err(HyperlightError::IntConversionFailure)
    }

    #[test]
    fn test_termination() -> Result<()> {
        let mut sandbox = SandboxBuilder::new().build()?;

        sandbox.register(
            "GetTimeSinceBootMicrosecond",
            get_time_since_boot_microsecond,
        )?;

        let loaded = sandbox.load_runtime()?;

        let run_wasm = get_test_file_path("RunWasm.aot")?;

        let mut loaded = loaded.load_module(run_wasm)?;

        let interrupt = loaded.interrupt_handle()?;

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            interrupt.kill();
        });

        let result = loaded.call_guest_function::<i32>("KeepCPUBusy", 10000i32);

        match result {
            Ok(_) => panic!("Expected error"),
            Err(e) => match e {
                HyperlightError::ExecutionCanceledByHost() => {}
                _ => panic!("Unexpected error: {:?}", e),
            },
        }

        // Verify sandbox is poisoned after interruption
        assert!(
            loaded.is_poisoned()?,
            "Sandbox should be poisoned after interruption"
        );

        Ok(())
    }

    #[test]
    fn test_sandbox_is_poisoned_after_interruption() -> Result<()> {
        let mut sandbox = SandboxBuilder::new().build()?;

        sandbox.register(
            "GetTimeSinceBootMicrosecond",
            get_time_since_boot_microsecond,
        )?;

        let loaded = sandbox.load_runtime()?;
        let run_wasm = get_test_file_path("RunWasm.aot")?;
        let mut loaded = loaded.load_module(run_wasm)?;

        // Verify sandbox is not poisoned initially
        assert!(
            !loaded.is_poisoned()?,
            "Sandbox should not be poisoned initially"
        );

        let interrupt = loaded.interrupt_handle()?;

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            interrupt.kill();
        });

        // This call will be interrupted
        let _ = loaded.call_guest_function::<i32>("KeepCPUBusy", 100000i32);

        // Verify sandbox is now poisoned
        assert!(
            loaded.is_poisoned()?,
            "Sandbox should be poisoned after interruption"
        );

        Ok(())
    }

    #[test]
    fn test_call_guest_function_fails_when_poisoned() -> Result<()> {
        let mut sandbox = SandboxBuilder::new().build()?;

        sandbox.register(
            "GetTimeSinceBootMicrosecond",
            get_time_since_boot_microsecond,
        )?;

        let loaded = sandbox.load_runtime()?;
        let run_wasm = get_test_file_path("RunWasm.aot")?;
        let mut loaded = loaded.load_module(run_wasm)?;

        let interrupt = loaded.interrupt_handle()?;

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            interrupt.kill();
        });

        // First call will be interrupted
        let _ = loaded.call_guest_function::<i32>("KeepCPUBusy", 100000i32);

        // Second call should fail with PoisonedSandbox
        let result = loaded.call_guest_function::<i32>("PrintOutput", 42i32);

        match result {
            Ok(_) => panic!("Expected PoisonedSandbox error"),
            Err(HyperlightError::PoisonedSandbox) => {
                // Expected error
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }

        Ok(())
    }

    #[test]
    fn test_snapshot_fails_when_poisoned() -> Result<()> {
        let mut sandbox = SandboxBuilder::new().build()?;

        sandbox.register(
            "GetTimeSinceBootMicrosecond",
            get_time_since_boot_microsecond,
        )?;

        let loaded = sandbox.load_runtime()?;
        let run_wasm = get_test_file_path("RunWasm.aot")?;
        let mut loaded = loaded.load_module(run_wasm)?;

        let interrupt = loaded.interrupt_handle()?;

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            interrupt.kill();
        });

        // Call will be interrupted, poisoning the sandbox
        let _ = loaded.call_guest_function::<i32>("KeepCPUBusy", 100000i32);

        // Snapshot should fail on poisoned sandbox
        let result = loaded.snapshot();

        match result {
            Ok(_) => panic!("Expected PoisonedSandbox error"),
            Err(HyperlightError::PoisonedSandbox) => {
                // Expected error
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }

        Ok(())
    }

    #[test]
    fn test_restore_recovers_poisoned_sandbox() -> Result<()> {
        let mut sandbox = SandboxBuilder::new().build()?;

        sandbox.register(
            "GetTimeSinceBootMicrosecond",
            get_time_since_boot_microsecond,
        )?;

        let loaded = sandbox.load_runtime()?;
        let run_wasm = get_test_file_path("RunWasm.aot")?;
        let mut loaded = loaded.load_module(run_wasm)?;

        // Take a snapshot before poisoning
        let snapshot = loaded.snapshot()?;

        let interrupt = loaded.interrupt_handle()?;

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            interrupt.kill();
        });

        // Call will be interrupted, poisoning the sandbox
        let _ = loaded.call_guest_function::<i32>("KeepCPUBusy", 100000i32);

        assert!(loaded.is_poisoned()?, "Sandbox should be poisoned");

        // Restore should recover the sandbox
        loaded.restore(snapshot)?;

        assert!(
            !loaded.is_poisoned()?,
            "Sandbox should not be poisoned after restore"
        );

        // Should be able to call guest functions again
        let result: i32 = loaded.call_guest_function("CalcFib", 10i32)?;
        assert_eq!(result, 55);

        Ok(())
    }

    #[test]
    fn test_unload_module_recovers_poisoned_sandbox() -> Result<()> {
        let mut sandbox = SandboxBuilder::new().build()?;

        sandbox.register(
            "GetTimeSinceBootMicrosecond",
            get_time_since_boot_microsecond,
        )?;

        let loaded = sandbox.load_runtime()?;
        let run_wasm = get_test_file_path("RunWasm.aot")?;
        let mut loaded = loaded.load_module(run_wasm)?;

        let interrupt = loaded.interrupt_handle()?;

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            interrupt.kill();
        });

        // Call will be interrupted, poisoning the sandbox
        let _ = loaded.call_guest_function::<i32>("KeepCPUBusy", 100000i32);

        assert!(loaded.is_poisoned()?, "Sandbox should be poisoned");

        // unload_module should recover the sandbox (it calls restore internally)
        let wasm_sandbox = loaded.unload_module()?;

        // Should be able to load a new module and call functions
        let helloworld_wasm = get_test_file_path("HelloWorld.aot")?;
        let mut new_loaded = wasm_sandbox.load_module(helloworld_wasm)?;

        assert!(
            !new_loaded.is_poisoned()?,
            "New sandbox should not be poisoned"
        );

        let result: i32 = new_loaded.call_guest_function("HelloWorld", "Test".to_string())?;
        assert_eq!(result, 0);

        Ok(())
    }

    #[test]
    fn test_load_module_file() {
        let sandboxes = get_test_wasm_sandboxes().unwrap();

        for sbox_test in sandboxes {
            let name = sbox_test.name;
            println!("test_load_module: {name}");
            let wasm_sandbox = sbox_test.sbox;

            let helloworld_wasm = get_test_file_path("HelloWorld.aot").unwrap();
            let mut loaded_wasm_sandbox = wasm_sandbox.load_module(helloworld_wasm).unwrap();
            let result: i32 = loaded_wasm_sandbox
                .call_guest_function("HelloWorld", "Message from Rust Test".to_string())
                .unwrap();

            // TODO: Validate the output from the Wasm Modules.
            println!("({name}) Result {:?}", result);
        }
    }

    #[test]
    fn test_load_from_snapshot() {
        let mut sandbox = SandboxBuilder::new().build().unwrap();
        sandbox
            .register(
                "GetTimeSinceBootMicrosecond",
                get_time_since_boot_microsecond,
            )
            .unwrap();
        let sb = sandbox.load_runtime().unwrap();

        let helloworld_wasm = get_test_file_path("HelloWorld.aot").unwrap();
        let runwasm_wasm = get_test_file_path("RunWasm.aot").unwrap();

        // load one module, and make sure that a function in it
        // can be called
        let mut lb1 = sb.load_module(helloworld_wasm).unwrap();
        let result: i32 = lb1
            .call_guest_function("HelloWorld", "Message from Rust Test".to_string())
            .unwrap();
        assert_eq!(result, 0);
        let snapshot = lb1.snapshot().unwrap();

        // load another module, and make sure that a function in
        // it can be called
        let sb = lb1.unload_module().unwrap();
        let mut lb2 = sb.load_module(runwasm_wasm).unwrap();
        let result: i32 = lb2.call_guest_function("CalcFib", 10i32).unwrap();
        assert_eq!(result, 55);

        // reload the first module via snapshot, and make sure the
        // original function can be called again
        let sb = lb2.unload_module().unwrap();
        let mut lb3 = sb.load_from_snapshot(snapshot).unwrap();
        let result: i32 = lb3
            .call_guest_function("HelloWorld", "Message from Rust Test".to_string())
            .unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_load_module_buffer() {
        let sandboxes = get_test_wasm_sandboxes().unwrap();

        for sbox_test in sandboxes {
            let name = sbox_test.name;
            println!("test_load_module: {name}");
            let wasm_sandbox = sbox_test.sbox;

            let wasm_module_buffer: Vec<u8> =
                std::fs::read(get_test_file_path("HelloWorld.aot").unwrap()).unwrap();
            let mut loaded_wasm_sandbox = wasm_sandbox
                .load_module_from_buffer(&wasm_module_buffer)
                .unwrap();
            let result: i32 = loaded_wasm_sandbox
                .call_guest_function("HelloWorld", "Message from Rust Test".to_string())
                .unwrap();

            // TODO: Validate the output from the Wasm Modules.
            println!("({name}) Result {:?}", result);
        }
    }

    pub(super) fn get_test_file_path(filename: &str) -> Result<String> {
        #[cfg(debug_assertions)]
        let config = "debug";
        #[cfg(not(debug_assertions))]
        let config = "release";
        let proj_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| {
            env::var_os("RUST_DIR_FOR_DEBUGGING_TESTS")
                .expect("Failed to get CARGO_MANIFEST_DIR  or RUST_DIR_FOR_DEBUGGING_TESTS env var")
        });

        let relative_path = "../../x64";

        let filename_path = Path::new(&proj_dir)
            .join(relative_path)
            .join(config)
            .join(filename);

        let full_path = filename_path
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        Ok(full_path)
    }

    struct SandboxTest {
        sbox: WasmSandbox,
        name: String,
    }

    fn get_test_wasm_sandboxes() -> Result<Vec<SandboxTest>> {
        let builder = SandboxBuilder::new()
            .with_guest_input_buffer_size(0x8000)
            .with_guest_output_buffer_size(0x8000)
            .with_guest_scratch_size(0x2000)
            .with_guest_heap_size(0x100000);

        let mut sandboxes: Vec<SandboxTest> = Vec::new();
        if is_hypervisor_present() {
            sandboxes.push(SandboxTest {
                sbox: builder.clone().build()?.load_runtime()?,
                name: "regular in-hypervisor".to_string(),
            });
        }

        Ok(sandboxes)
    }
}
