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

use hyperlight_host::func::{HostFunction, ParameterTuple, Registerable, SupportedReturnType};
use hyperlight_host::sandbox::SandboxConfiguration;
use hyperlight_host::{
    GuestBinary, HostFunctions, HyperlightError, Result, Sandbox, is_hypervisor_present,
};

use super::proto_wasm_sandbox::ProtoWasmSandbox;
use super::snapshot::{Snapshot, SnapshotTarget};

// use large minimum scratch/heap/input data sizes
// to deal with the size of wasmtime/wasi-libc aot artifacts
pub const MIN_SCRATCH_SIZE: usize = 2 * 1024 * 1024;
pub const MIN_INPUT_DATA_SIZE: usize = 192 * 1024;
pub const MIN_HEAP_SIZE: u64 = 1024 * 1024;

/// Builder state for a new sandbox.
#[derive(Clone, Debug)]
pub struct NewSandbox;

/// Builder state for a sandbox built from a snapshot.
pub struct SnapshotSource<T: SnapshotTarget> {
    snapshot: Snapshot<T>,
    host_functions: HostFunctions,
}

/// Builds a new sandbox or one from a snapshot.
#[derive(Clone)]
pub struct SandboxBuilder<S = NewSandbox> {
    source: S,
    config: SandboxConfiguration,
    host_print_fn: Option<HostFunction<i32, (String,)>>,
}

impl<S> SandboxBuilder<S> {
    fn with_source(source: S, config: SandboxConfiguration) -> Self {
        Self {
            source,
            config,
            host_print_fn: None,
        }
    }

    /// Enable debugging for the guest
    /// This will allow the guest to be natively debugged using GDB or other debugging tools
    ///
    /// # Example:
    /// ```rust
    /// use hyperlight_wasm::SandboxBuilder;
    /// let sandbox = SandboxBuilder::new()
    ///    .with_debugging_enabled(8080) // Enable debugging on port 8080
    ///    .build()
    ///    .expect("Failed to build sandbox");
    /// ```
    /// # Note:
    /// This feature is only available when the `gdb` feature is enabled.
    /// If the `gdb` feature is not enabled, this method will have no effect.
    #[cfg(gdb)]
    pub fn with_debugging_enabled(mut self, port: u16) -> Self {
        let debug_info = hyperlight_host::sandbox::config::DebugInfo { port };
        self.config.set_guest_debug_info(debug_info);

        self
    }

    /// Set the host print function
    pub fn with_host_print_fn(
        mut self,
        host_print_fn: impl Into<HostFunction<i32, (String,)>>,
    ) -> Self {
        self.host_print_fn = Some(host_print_fn.into());
        self
    }

    /// Enable or disable crashdump generation for the sandbox
    /// When enabled, core dumps will be generated when the guest crashes
    /// This requires the `crashdump` feature to be enabled
    #[cfg(feature = "crashdump")]
    pub fn with_crashdump_enabled(mut self, enabled: bool) -> Self {
        self.config.set_guest_core_dump(enabled);
        self
    }
}

impl SandboxBuilder<NewSandbox> {
    /// Configure a sandbox using the embedded runtime.
    pub fn new() -> Self {
        let mut config = SandboxConfiguration::default();
        config.set_h2g_pool_pages(MIN_INPUT_DATA_SIZE / hyperlight_common::vmem::PAGE_SIZE);
        config.set_heap_size(MIN_HEAP_SIZE);
        config.set_scratch_size(MIN_SCRATCH_SIZE);
        Self::with_source(NewSandbox, config)
    }

    /// Set the guest output buffer size
    #[deprecated(
        since = "0.16.0",
        note = "use with_g2h_{pool_pages,buffer_size,queue_size}"
    )]
    pub fn with_guest_output_buffer_size(mut self, guest_output_buffer_size: usize) -> Self {
        self.config
            .set_g2h_pool_pages(guest_output_buffer_size / hyperlight_common::vmem::PAGE_SIZE + 1);
        self
    }

    /// Set the guest input buffer size
    /// This is the size of the buffer that the guest can write to
    /// to send data to the host
    /// The host can read from this buffer
    /// The guest can write to this buffer
    #[deprecated(
        since = "0.16.0",
        note = "use with_h2g_{pool_pages,buffer_size,queue_size}"
    )]
    pub fn with_guest_input_buffer_size(mut self, guest_input_buffer_size: usize) -> Self {
        self.config
            .set_h2g_pool_pages(guest_input_buffer_size / hyperlight_common::vmem::PAGE_SIZE + 1);
        self
    }

    /// Set the maximum number of guest pages used for Guest2Host buffers
    pub fn with_g2h_pool_pages(mut self, pages: usize) -> Self {
        self.config.set_g2h_pool_pages(pages);
        self
    }

    /// Set the maximum number of in-flight Guest2Host descriptors
    pub fn with_g2h_queue_size(mut self, entries: usize) -> Self {
        self.config.set_g2h_queue_size(entries);
        self
    }

    /// Set the size of each in-flight GuestToHost buffer
    pub fn with_g2h_buffer_size(mut self, size: usize) -> Self {
        self.config.set_g2h_buffer_size(size);
        self
    }

    /// Set the maximum number of guest pages used for Host2Guest buffers
    pub fn with_h2g_pool_pages(mut self, pages: usize) -> Self {
        if pages > (MIN_INPUT_DATA_SIZE / hyperlight_common::vmem::PAGE_SIZE) {
            self.config.set_h2g_pool_pages(pages);
        }
        self
    }

    /// Set the maximum number of in-flight Host2Guest descriptors
    pub fn with_h2g_queue_size(mut self, entries: usize) -> Self {
        self.config.set_h2g_queue_size(entries);
        self
    }

    /// Set the size of each in-flight Host2Guest buffer
    pub fn with_h2g_buffer_size(mut self, size: usize) -> Self {
        self.config.set_h2g_buffer_size(size);
        self
    }

    /// Set the guest scratch size in bytes.
    /// The scratch region provides writable memory for the guest, including the
    /// dynamically-sized stack. Increase this if your guest code needs deep
    /// recursion or large local variables.
    /// Values smaller than the default (288 KiB) are ignored.
    pub fn with_guest_scratch_size(mut self, guest_scratch_size: usize) -> Self {
        if guest_scratch_size > MIN_SCRATCH_SIZE {
            self.config.set_scratch_size(guest_scratch_size);
        }
        self
    }

    /// Set the guest heap size
    /// This is the size of the heap that code executing in the guest can use.
    /// If this value is too small then the guest will fail, usually with a malloc failed error
    /// The default (and minimum) value for this is set to the value of the MIN_HEAP_SIZE const.
    pub fn with_guest_heap_size(mut self, guest_heap_size: u64) -> Self {
        if guest_heap_size > MIN_HEAP_SIZE {
            self.config.set_heap_size(guest_heap_size);
        }
        self
    }

    /// Build the ProtoWasmSandbox
    pub fn build(self) -> Result<ProtoWasmSandbox> {
        if !is_hypervisor_present() {
            return Err(HyperlightError::NoHypervisorFound());
        }

        let guest_binary = GuestBinary::Buffer(super::WASM_RUNTIME.to_vec());

        let mut proto_wasm_sandbox = ProtoWasmSandbox::new(Some(self.config), guest_binary)?;
        if let Some(host_print_fn) = self.host_print_fn {
            proto_wasm_sandbox.register_print(host_print_fn)?;
        }
        Ok(proto_wasm_sandbox)
    }
}

/// ```compile_fail
/// use hyperlight_wasm::{LoadedWasmSandbox, SandboxBuilder, Snapshot};
///
/// fn restore(snapshot: Snapshot<LoadedWasmSandbox>) {
///     let _builder = SandboxBuilder::from_snapshot(snapshot).with_guest_heap_size(1 << 21);
/// }
/// ```
#[cfg(doctest)]
struct SnapshotBuilderHasNoLayoutSetters;

impl<T: SnapshotTarget> SandboxBuilder<SnapshotSource<T>> {
    /// Create a builder that builds a sandbox from `snapshot`.
    ///
    /// Register every host function the guest imports before calling
    /// [`build`](Self::build). Layout setters are unavailable because the snapshot
    /// fixes the memory layout.
    pub fn from_snapshot(snapshot: Snapshot<T>) -> Self {
        Self::with_source(
            SnapshotSource {
                snapshot,
                host_functions: HostFunctions::default(),
            },
            SandboxConfiguration::default(),
        )
    }

    /// Register a host function the guest imports.
    pub fn register<Args: ParameterTuple, Output: SupportedReturnType>(
        &mut self,
        name: impl AsRef<str>,
        host_func: impl Into<HostFunction<Output, Args>>,
    ) -> Result<()> {
        self.register_host_function(name.as_ref(), host_func)
    }

    /// Build the sandbox from the snapshot.
    ///
    /// Fails if a host function the guest imports is missing or has a different
    /// signature.
    pub fn build(mut self) -> Result<T> {
        if let Some(host_print_fn) = self.host_print_fn {
            self.source
                .host_functions
                .register_host_function("HostPrint", host_print_fn)?;
        }
        let inner = Sandbox::from_snapshot(
            self.source.snapshot.inner(),
            self.source.host_functions,
            Some(self.config),
        )?;
        self.source.snapshot.build(inner)
    }
}

impl<T: SnapshotTarget> Registerable for SandboxBuilder<SnapshotSource<T>> {
    fn register_host_function<Args: ParameterTuple, Output: SupportedReturnType>(
        &mut self,
        name: &str,
        host_func: impl Into<HostFunction<Output, Args>>,
    ) -> Result<()> {
        self.source
            .host_functions
            .register_host_function(name, host_func)
    }
}

impl Default for SandboxBuilder<NewSandbox> {
    fn default() -> Self {
        Self::new()
    }
}
