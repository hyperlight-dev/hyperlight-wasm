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

use hyperlight_host::func::HostFunction;
use hyperlight_host::sandbox::SandboxConfiguration;
use hyperlight_host::{GuestBinary, HyperlightError, Result, is_hypervisor_present};

use super::proto_wasm_sandbox::ProtoWasmSandbox;

// use unreasonably large minimum stack/heap/input data sizes for now to
// deal with the size of wasmtime/wasi-libc aot artifacts
pub const MIN_STACK_SIZE: u64 = 64 * 1024;
pub const MIN_INPUT_DATA_SIZE: usize = 192 * 1024;
pub const MIN_HEAP_SIZE: u64 = 1024 * 1024;

/// A builder for WasmSandbox
#[derive(Clone)]
pub struct SandboxBuilder {
    config: SandboxConfiguration,
    host_print_fn: Option<HostFunction<i32, (String,)>>,
}

impl SandboxBuilder {
    /// Create a new SandboxBuilder
    pub fn new() -> Self {
        let mut config: SandboxConfiguration = Default::default();
        config.set_input_data_size(MIN_INPUT_DATA_SIZE);
        config.set_heap_size(MIN_HEAP_SIZE);

        Self {
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

    /// Set the guest output buffer size
    pub fn with_guest_output_buffer_size(mut self, guest_output_buffer_size: usize) -> Self {
        self.config.set_output_data_size(guest_output_buffer_size);
        self
    }

    /// Set the guest input buffer size
    /// This is the size of the buffer that the guest can write to
    /// to send data to the host
    /// The host can read from this buffer
    /// The guest can write to this buffer
    pub fn with_guest_input_buffer_size(mut self, guest_input_buffer_size: usize) -> Self {
        if guest_input_buffer_size > MIN_INPUT_DATA_SIZE {
            self.config.set_input_data_size(guest_input_buffer_size);
        }
        self
    }

    /// Set the guest stack size
    /// This is the size of the stack that code executing in the guest can use.
    /// If this value is too small then the guest will fail with a stack overflow error
    /// The default value (and minimum) is set to the value of the MIN_STACK_SIZE const.
    pub fn with_guest_stack_size(mut self, guest_stack_size: u64) -> Self {
        if guest_stack_size > MIN_STACK_SIZE {
            self.config.set_stack_size(guest_stack_size);
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

    /// Enable or disable crashdump generation for the sandbox
    /// When enabled, core dumps will be generated when the guest crashes
    /// This requires the `crashdump` feature to be enabled
    #[cfg(feature = "crashdump")]
    pub fn with_crashdump_enabled(mut self, enabled: bool) -> Self {
        self.config.set_guest_core_dump(enabled);
        self
    }

    /// Set the size of the memory buffer that is made available
    /// for serialising host function definitions the minimum value
    /// is MIN_FUNCTION_DEFINITION_SIZE
    pub fn with_function_definition_size(mut self, size: usize) -> Self {
        self.config.set_host_function_definition_size(size);
        self
    }

    /// Build the ProtoWasmSandbox
    pub fn build(self) -> Result<ProtoWasmSandbox> {
        if !is_hypervisor_present() {
            return Err(HyperlightError::NoHypervisorFound());
        }

        let guest_binary = GuestBinary::Buffer(&super::WASM_RUNTIME);

        let mut proto_wasm_sandbox = ProtoWasmSandbox::new(Some(self.config), guest_binary)?;
        if let Some(host_print_fn) = self.host_print_fn {
            proto_wasm_sandbox.register_print(host_print_fn)?;
        }
        Ok(proto_wasm_sandbox)
    }
}

impl Default for SandboxBuilder {
    fn default() -> Self {
        Self::new()
    }
}
