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

// use large minimum scratch/heap/input data sizes
// to deal with the size of wasmtime/wasi-libc aot artifacts
pub const MIN_SCRATCH_SIZE: usize = 2 * 1024 * 1024;
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
        config.set_h2g_pool_pages(MIN_INPUT_DATA_SIZE / hyperlight_common::vmem::PAGE_SIZE);
        config.set_heap_size(MIN_HEAP_SIZE);
        config.set_scratch_size(MIN_SCRATCH_SIZE);

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
        if guest_input_buffer_size > MIN_INPUT_DATA_SIZE {
            self.config.set_h2g_pool_pages(
                guest_input_buffer_size / hyperlight_common::vmem::PAGE_SIZE + 1,
            );
        }
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
        if pages > (MIN_INPUT_DATA_SIZE / hyperlight_common::vmem::PAGE_SIZE + 1) {
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

    /// Enable or disable crashdump generation for the sandbox
    /// When enabled, core dumps will be generated when the guest crashes
    /// This requires the `crashdump` feature to be enabled
    #[cfg(feature = "crashdump")]
    pub fn with_crashdump_enabled(mut self, enabled: bool) -> Self {
        self.config.set_guest_core_dump(enabled);
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

impl Default for SandboxBuilder {
    fn default() -> Self {
        Self::new()
    }
}
