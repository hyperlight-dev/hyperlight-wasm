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

use std::time::Duration;

#[cfg(all(target_os = "windows", not(debug_assertions)))]
use hyperlight_host::new_error;
use hyperlight_host::sandbox::SandboxConfiguration;
use hyperlight_host::{
    GuestBinary, HyperlightError, Result, SandboxRunOptions, is_hypervisor_present,
};

use super::proto_wasm_sandbox::ProtoWasmSandbox;
use crate::HostPrintFn;

// use unreasonably large minimum stack/heap/input data sizes for now to
// deal with the size of wasmtime/wasi-libc aot artifacts
pub const MIN_STACK_SIZE: u64 = 64 * 1024;
pub const MIN_INPUT_DATA_SIZE: usize = 192 * 1024;
pub const MIN_HEAP_SIZE: u64 = 1024 * 1024;
pub const MIN_GUEST_ERROR_BUFFER_SIZE: usize = 1024;

/// A builder for WasmSandbox
#[derive(Clone)]
pub struct SandboxBuilder<'a> {
    config: SandboxConfiguration,
    opts: Option<SandboxRunOptions>,
    host_print_fn: Option<HostPrintFn<'a>>,
    #[cfg(all(target_os = "windows", debug_assertions))]
    guest_path: Option<String>,
}

impl<'a> SandboxBuilder<'a> {
    /// Create a new SandboxBuilder
    pub fn new() -> Self {
        let mut config: SandboxConfiguration = Default::default();
        config.set_input_data_size(MIN_INPUT_DATA_SIZE);
        config.set_heap_size(MIN_HEAP_SIZE);

        Self {
            config,
            opts: None,
            host_print_fn: None,
            #[cfg(all(target_os = "windows", debug_assertions))]
            guest_path: None,
        }
    }

    /// Set the host print function
    pub fn with_host_print_fn(mut self, host_print_fn: HostPrintFn<'a>) -> Self {
        self.host_print_fn = Some(host_print_fn);
        self
    }

    /// Set the guest error buffer size
    /// The default (and minimum) value for this is set to the value of the MIN_GUEST_ERROR_BUFFER_SIZE const.
    pub fn with_guest_error_buffer_size(mut self, guest_error_buffer_size: usize) -> Self {
        if guest_error_buffer_size > MIN_GUEST_ERROR_BUFFER_SIZE {
            self.config
                .set_guest_error_buffer_size(MIN_GUEST_ERROR_BUFFER_SIZE);
        }
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

    /// Set the host function definition buffer size
    /// This is the size of the buffer that the host can write
    /// details of the host functions that the guest can call
    ///
    pub fn with_host_function_buffer_size(mut self, host_function_buffer_size: usize) -> Self {
        self.config
            .set_host_function_definition_size(host_function_buffer_size);
        self
    }

    /// Set the host exception buffer size
    /// This is the size of the buffer that the host can write to with
    /// details of any exceptions/errors/panics that are thrown during host function execution
    ///
    /// It allows details of any exception or error that occurred in a host function called from the guest
    /// to be captured and returned to the host via the guest as it is not possible to transparently return exception/error/panic
    /// details from a host function to the guest as the guest/host boundary is a hard boundary
    pub fn with_host_exception_buffer_size(mut self, host_exception_buffer_size: usize) -> Self {
        self.config
            .set_host_exception_size(host_exception_buffer_size);
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

    /// Set the guest panic context buffer size
    pub fn with_guest_panic_context_buffer_size(mut self, guest_panic_buffer_size: usize) -> Self {
        self.config
            .set_guest_panic_context_buffer_size(guest_panic_buffer_size);
        self
    }

    /// Set the max execution time for the guest function call
    /// If the guest function call takes longer than this time then the guest function call will be terminated
    /// and the guest function will return an error
    pub fn with_guest_function_call_max_execution_time_millis(
        mut self,
        guest_function_call_max_execution_time: u64,
    ) -> Self {
        self.config.set_max_execution_time(Duration::from_millis(
            guest_function_call_max_execution_time,
        ));
        self
    }

    /// Set the max time to wait for cancellation of a guest function call
    /// If cancellation of the guest function call takes longer than this time then the guest function call will be terminated
    /// and the guest function will return an error
    pub fn with_guest_function_call_max_cancel_wait_millis(
        mut self,
        guest_function_call_max_cancel_wait_time: u64,
    ) -> Self {
        self.config
            .set_max_execution_cancel_wait_time(Duration::from_millis(
                guest_function_call_max_cancel_wait_time,
            ));
        self
    }

    /// Get the current configuration
    pub fn get_config(&self) -> &SandboxConfiguration {
        &self.config
    }

    /// Build the ProtoWasmSandbox
    pub fn build(self) -> Result<ProtoWasmSandbox> {
        let guest_binary = match self.opts {
            #[cfg(target_os = "windows")]
            Some(SandboxRunOptions::RunInProcess(debug)) => match debug {
                false => GuestBinary::Buffer(super::WASM_RUNTIME.to_vec()),
                true => {
                    #[cfg(debug_assertions)]
                    {
                        let guest_path = self.guest_path.unwrap();
                        GuestBinary::FilePath(guest_path)
                    }
                    #[cfg(not(debug_assertions))]
                    {
                        return Err(new_error!("Debugging is only supported in debug builds "));
                    }
                }
            },
            _ => {
                if !is_hypervisor_present() {
                    return Err(HyperlightError::NoHypervisorFound());
                }
                GuestBinary::Buffer(super::WASM_RUNTIME.to_vec())
            }
        };

        let proto_wasm_sandbox = ProtoWasmSandbox::new(
            Some(self.config),
            self.opts,
            guest_binary,
            self.host_print_fn,
        )?;
        Ok(proto_wasm_sandbox)
    }
}

impl<'a> Default for SandboxBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}
