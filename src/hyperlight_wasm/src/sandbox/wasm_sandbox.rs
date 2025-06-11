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

use hyperlight_host::func::call_ctx::MultiUseGuestCallContext;
use hyperlight_host::sandbox::Callable;
use hyperlight_host::sandbox_state::sandbox::{EvolvableSandbox, Sandbox};
use hyperlight_host::sandbox_state::transition::MultiUseContextCallback;
use hyperlight_host::{MultiUseSandbox, Result, new_error};

use super::loaded_wasm_sandbox::LoadedWasmSandbox;
use crate::sandbox::metrics::{
    METRIC_ACTIVE_WASM_SANDBOXES, METRIC_SANDBOX_LOADS, METRIC_TOTAL_WASM_SANDBOXES,
};

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
    inner: Option<MultiUseSandbox>,
}

impl Sandbox for WasmSandbox {}

impl WasmSandbox {
    /// Create a new WasmSandBox from a `MultiUseSandbox`.
    /// This function should be used to create a new `WasmSandbox` from a ProtoWasmSandbox.
    /// The difference between this function and creating  a `WasmSandbox` directly is that
    /// this function will increment the metrics for the number of `WasmSandbox`es in the system.
    pub(super) fn new(inner: MultiUseSandbox) -> Self {
        metrics::gauge!(METRIC_ACTIVE_WASM_SANDBOXES).increment(1);
        metrics::counter!(METRIC_TOTAL_WASM_SANDBOXES).increment(1);
        WasmSandbox { inner: Some(inner) }
    }

    /// Load a Wasm module at the given path into the sandbox and return a `LoadedWasmSandbox`
    /// able to execute code in the loaded Wasm Module.
    ///
    /// Before you can call guest functions in the sandbox, you must call
    /// this function and use the returned value to call guest functions.
    pub fn load_module(self, file: impl AsRef<Path>) -> Result<LoadedWasmSandbox> {
        let wasm_bytes = std::fs::read(file)?;
        self.load_module_inner(wasm_bytes)
    }

    /// Load a Wasm module from a buffer of bytes into the sandbox and return a `LoadedWasmSandbox`
    /// able to execute code in the loaded Wasm Module.
    ///
    /// Before you can call guest functions in the sandbox, you must call
    /// this function and use the returned value to call guest functions.
    pub fn load_module_from_buffer(self, buffer: &[u8]) -> Result<LoadedWasmSandbox> {
        self.load_module_inner(buffer.to_vec())
    }

    fn load_module_inner(mut self, wasm_bytes: Vec<u8>) -> Result<LoadedWasmSandbox> {
        let func = Box::new(move |call_ctx: &mut MultiUseGuestCallContext| {
            let len = wasm_bytes.len() as i32;
            let res: i32 = call_ctx.call("LoadWasmModule", (wasm_bytes, len))?;
            if res != 0 {
                return Err(new_error!(
                    "LoadWasmModule Failed with error code {:?}",
                    res
                ));
            }
            Ok(())
        });

        let transition_func = MultiUseContextCallback::from(func);

        match self.inner.take() {
            Some(sbox) => {
                let new_sbox: MultiUseSandbox = sbox.evolve(transition_func)?;
                metrics::counter!(METRIC_SANDBOX_LOADS).increment(1);
                LoadedWasmSandbox::new(new_sbox)
            }
            None => Err(new_error!("WasmSandbox is None, cannot load module")),
        }
    }
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
    use crate::sandbox::sandbox_builder::SandboxBuilder;

    #[test]
    fn test_new_sandbox() -> Result<()> {
        let _sandbox = SandboxBuilder::new().build()?;
        Ok(())
    }

    fn get_time_since_boot_microsecond() -> Result<i64> {
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

        let run_wasm = get_test_file_path("RunWasm.wasm")?;

        let mut loaded = loaded.load_module(run_wasm)?;

        let result = loaded.call_guest_function::<i32>("KeepCPUBusy", 10000i32);

        match result {
            Ok(_) => panic!("Expected error"),
            Err(e) => match e {
                HyperlightError::ExecutionCanceledByHost() => {}
                _ => panic!("Unexpected error: {:?}", e),
            },
        }

        Ok(())
    }

    #[test]
    fn test_load_module_file() {
        let sandboxes = get_test_wasm_sandboxes().unwrap();

        for sbox_test in sandboxes {
            let name = sbox_test.name;
            println!("test_load_module: {name}");
            let wasm_sandbox = sbox_test.sbox;

            let helloworld_wasm = get_test_file_path("HelloWorld.wasm").unwrap();
            let mut loaded_wasm_sandbox = wasm_sandbox.load_module(helloworld_wasm).unwrap();
            let result: i32 = loaded_wasm_sandbox
                .call_guest_function("HelloWorld", "Message from Rust Test".to_string())
                .unwrap();

            // TODO: Validate the output from the Wasm Modules.
            println!("({name}) Result {:?}", result);
        }
    }

    #[test]
    fn test_load_module_buffer() {
        let sandboxes = get_test_wasm_sandboxes().unwrap();

        for sbox_test in sandboxes {
            let name = sbox_test.name;
            println!("test_load_module: {name}");
            let wasm_sandbox = sbox_test.sbox;

            let wasm_module_buffer: Vec<u8> =
                std::fs::read(get_test_file_path("HelloWorld.wasm").unwrap()).unwrap();
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

    fn get_test_file_path(filename: &str) -> Result<String> {
        #[cfg(debug_assertions)]
        let config = "debug";
        #[cfg(not(debug_assertions))]
        let config = "release";
        let proj_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| {
            env::var_os("RUST_DIR_FOR_DEBUGGING_TESTS")
                .expect("Failed to get CARGO_MANIFEST_DIR  or RUST_DIR_FOR_DEBUGGING_TESTS env var")
        });

        let relative_path = if filename == "wasm_runtime" {
            "redist"
        } else {
            "../../x64"
        };

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
            .with_guest_stack_size(0x2000)
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
