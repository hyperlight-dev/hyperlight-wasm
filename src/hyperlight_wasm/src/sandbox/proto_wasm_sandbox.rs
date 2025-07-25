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
#[cfg(all(feature = "seccomp", target_os = "linux"))]
use hyperlight_host::sandbox::ExtraAllowedSyscall;
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
}

impl Registerable for ProtoWasmSandbox {
    fn register_host_function<Args: ParameterTuple, Output: SupportedReturnType>(
        &mut self,
        name: &str,
        hf: impl Into<HostFunction<Output, Args>>,
    ) -> Result<()> {
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))
            .and_then(|sb| sb.register(name, hf))
    }
    #[cfg(all(feature = "seccomp", target_os = "linux"))]
    fn register_host_function_with_syscalls<Args: ParameterTuple, Output: SupportedReturnType>(
        &mut self,
        name: &str,
        hf: impl Into<HostFunction<Output, Args>>,
        eas: Vec<ExtraAllowedSyscall>,
    ) -> Result<()> {
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))
            .and_then(|sb| sb.register_host_function_with_syscalls(name, hf, eas))
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
        Ok(Self { inner: Some(inner) })
    }

    /// Load the Wasm runtime into the sandbox and return a `WasmSandbox`
    /// that can be cached and used to load a Wasm module resulting in a `LoadedWasmSandbox`
    ///
    /// The `LoadedWasmSandbox` can be reverted to a `WasmSandbox` by calling the `unload_runtime` method.
    /// The returned `WasmSandbox` can be then be cached and used to load a different Wasm module.
    ///
    pub fn load_runtime(mut self) -> Result<WasmSandbox> {
        let mut sandbox = match self.inner.take() {
            Some(s) => s.evolve()?,
            None => return Err(new_error!("No inner sandbox found.")),
        };

        let res: i32 = sandbox.call_guest_function_by_name("InitWasmRuntime", ())?;
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
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))?
            .register(name, host_func)
    }

    /// Register the given host function `host_func` with `self` along with a vec of syscalls the
    /// function requires and return `Ok` if the registration succeeded, and a descriptive `Err` if
    /// it didn't.
    #[cfg(all(feature = "seccomp", target_os = "linux"))]
    pub fn register_with_extra_allowed_syscalls<
        Args: ParameterTuple,
        Output: SupportedReturnType,
    >(
        &mut self,
        name: impl AsRef<str>,
        host_func: impl Into<HostFunction<Output, Args>>,
        extra_allowed_syscalls: impl IntoIterator<Item = ExtraAllowedSyscall>,
    ) -> Result<()> {
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))?
            .register_with_extra_allowed_syscalls(name, host_func, extra_allowed_syscalls)
    }

    /// Register the given host printing function `print_func` with `self`.
    /// Return `Ok` if the registration succeeded, and a descriptive `Err` otherwise.
    pub fn register_print(
        &mut self,
        print_func: impl Into<HostFunction<i32, (String,)>>,
    ) -> Result<()> {
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))?
            .register_print(print_func)
    }

    /// Register the given host printing function `print_func` with `self` along with a
    /// vec of syscalls the function requires.
    /// Return `Ok` if the registration succeeded, and a descriptive `Err` otherwise.
    #[cfg(all(feature = "seccomp", target_os = "linux"))]
    pub fn register_print_with_extra_allowed_syscalls(
        &mut self,
        print_func: impl Into<HostFunction<i32, (String,)>>,
        extra_allowed_syscalls: impl IntoIterator<Item = ExtraAllowedSyscall>,
    ) -> Result<()> {
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))?
            .register_print_with_extra_allowed_syscalls(print_func, extra_allowed_syscalls)
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
