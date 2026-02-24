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

use hyperlight_common::flatbuffer_wrappers::function_types::{ParameterType, ReturnType};
use hyperlight_common::flatbuffer_wrappers::host_function_definition::HostFunctionDefinition;
use hyperlight_common::flatbuffer_wrappers::host_function_details::HostFunctionDetails;
use hyperlight_host::func::{HostFunction, ParameterTuple, Registerable, SupportedReturnType};
use hyperlight_host::sandbox::config::SandboxConfiguration;
use hyperlight_host::{GuestBinary, Result, UninitializedSandbox, new_error};

use super::metrics::{METRIC_ACTIVE_PROTO_WASM_SANDBOXES, METRIC_TOTAL_PROTO_WASM_SANDBOXES};
use super::sandbox_builder::SandboxBuilder;
use super::wasm_sandbox::WasmSandbox;
use crate::build_info::BuildInfo;

/// A Hyperlight Sandbox with no Wasm run time loaded and no guest module code loaded.
/// This is used to register new host functions that can be called by guest code.
///
/// Once all guest functions have been loaded you can call `load_runtime to load the Wasm runtime and get a `WasmSandbox`.
///
/// With that `WasmSandbox` you can load a Wasm module through the `load_module` method and get a `LoadedWasmSandbox` which can then execute functions defined in the Wasm module.
pub struct ProtoWasmSandbox {
    pub(super) inner: Option<UninitializedSandbox>,
    /// Tracks registered host function definitions for pushing to the guest at load time
    host_function_definitions: Vec<HostFunctionDefinition>,
}

impl Registerable for ProtoWasmSandbox {
    fn register_host_function<Args: ParameterTuple, Output: SupportedReturnType>(
        &mut self,
        name: &str,
        hf: impl Into<HostFunction<Output, Args>>,
    ) -> Result<()> {
        // Track the host function definition for pushing to guest at load time
        self.host_function_definitions.push(HostFunctionDefinition {
            function_name: name.to_string(),
            parameter_types: Some(Args::TYPE.to_vec()),
            return_type: Output::TYPE,
        });

        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))
            .and_then(|sb| sb.register(name, hf))
    }
}

impl ProtoWasmSandbox {
    /// Create a new sandbox complete with no Wasm runtime and no end-user
    /// code loaded. Since there's no user code or runtime loaded, the returned
    /// sandbox cannot execute anything.
    ///
    /// The returned `WasmSandbox` can be cached and later used to load a Wasm module.
    ///
    /// If you'd like to restrict the (wall-clock) execution time for any guest function called in the
    /// `LoadedWasmSandbox` returned from the `load_module` method on the `WasmSandbox`, you can
    /// use the `max_execution_time` and `max_wait_for_cancellation`
    /// fields in the `SandboxConfiguration` struct.
    pub(super) fn new(
        cfg: Option<SandboxConfiguration>,
        guest_binary: GuestBinary,
    ) -> Result<Self> {
        BuildInfo::log();
        let inner = UninitializedSandbox::new(guest_binary, cfg)?;
        metrics::gauge!(METRIC_ACTIVE_PROTO_WASM_SANDBOXES).increment(1);
        metrics::counter!(METRIC_TOTAL_PROTO_WASM_SANDBOXES).increment(1);

        // HostPrint is always registered by UninitializedSandbox, so include it by default
        let host_function_definitions = vec![HostFunctionDefinition {
            function_name: "HostPrint".to_string(),
            parameter_types: Some(vec![ParameterType::String]),
            return_type: ReturnType::Int,
        }];

        Ok(Self {
            inner: Some(inner),
            host_function_definitions,
        })
    }

    /// Load the Wasm runtime into the sandbox and return a `WasmSandbox`
    /// that can be cached and used to load a Wasm module resulting in a `LoadedWasmSandbox`
    ///
    /// The `LoadedWasmSandbox` can be reverted to a `WasmSandbox` by calling the `unload_runtime` method.
    /// The returned `WasmSandbox` can be then be cached and used to load a different Wasm module.
    ///
    pub fn load_runtime(mut self) -> Result<WasmSandbox> {
        // Serialize host function definitions to push to the guest during InitWasmRuntime
        let host_function_definitions = HostFunctionDetails {
            host_functions: Some(std::mem::take(&mut self.host_function_definitions)),
        };

        let host_function_definitions_bytes: Vec<u8> = (&host_function_definitions)
            .try_into()
            .map_err(|e| new_error!("Failed to serialize host function details: {:?}", e))?;

        let mut sandbox = match self.inner.take() {
            Some(s) => s.evolve()?,
            None => return Err(new_error!("No inner sandbox found.")),
        };

        // Pass host function definitions to the guest as a parameter
        let res: i32 = sandbox.call("InitWasmRuntime", (host_function_definitions_bytes,))?;
        if res != 0 {
            return Err(new_error!(
                "InitWasmRuntime Failed  with error code {:?}",
                res
            ));
        }

        WasmSandbox::new(sandbox)
    }

    /// Register the given host function `host_func` with `self` under
    /// the given `name`. Return `Ok` if the registration succeeded, and a
    /// descriptive `Err` otherwise.
    pub fn register<Args: ParameterTuple, Output: SupportedReturnType>(
        &mut self,
        name: impl AsRef<str>,
        host_func: impl Into<HostFunction<Output, Args>>,
    ) -> Result<()> {
        // Track the host function definition for pushing to guest at load time
        self.host_function_definitions.push(HostFunctionDefinition {
            function_name: name.as_ref().to_string(),
            parameter_types: Some(Args::TYPE.to_vec()),
            return_type: Output::TYPE,
        });

        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))?
            .register(name, host_func)
    }

    /// Register the given host printing function `print_func` with `self`.
    /// Return `Ok` if the registration succeeded, and a descriptive `Err` otherwise.
    pub fn register_print(
        &mut self,
        print_func: impl Into<HostFunction<i32, (String,)>>,
    ) -> Result<()> {
        // HostPrint definition is already tracked from new() since
        // UninitializedSandbox always registers a default HostPrint.
        // This method only replaces the implementation, not the definition.
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))?
            .register_print(print_func)
    }
}

impl std::fmt::Debug for ProtoWasmSandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtoWasmSandbox").finish()
    }
}

impl Drop for ProtoWasmSandbox {
    fn drop(&mut self) {
        metrics::gauge!(METRIC_ACTIVE_PROTO_WASM_SANDBOXES).decrement(1);
    }
}

/// Create a new sandbox with a default configuration with a Wasm runtime but no end-user
/// code loaded. Since there's no user code loaded, the returned
/// sandbox cannot execute anything.
///
/// It can be used to register new host functions that can be called by guest code.
///
/// Once all guest functions have been loaded you can call `load_runtime` to load the Wasm runtme and get a `WasmSandbox`.
///
/// The default configuration is as follows:
///
/// * The Hyperlight default Host Print implementation is used.
/// * The Sandbox will attempt to run in a hypervisor and will fail if no Hypervisor is available.
/// * The Sandbox will have  a stack size of 8K and a heap size of 64K
/// * All other Hyperlight configuration values will be set to their defaults.
///
/// This will result in a memory footprint for the VM backing the Sandbox of approximately 434K
///
/// Use the `load_runtime` method to
/// load the Wasm runtime and convert the returned `ProtoWasmSandbox`
/// into a `WasmSandbox` that can have a Wasm module loaded returning a `LoadedWasmSandbox` that can be used to call Wasm functions in the guest.
impl Default for ProtoWasmSandbox {
    fn default() -> Self {
        SandboxBuilder::new().build().unwrap()
    }
}
