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

use hyperlight_host::func::call_ctx::MultiUseGuestCallContext;
use hyperlight_host::func::host_functions::{HostFunctionDefinition, Registerable};
use hyperlight_host::func::{
    HostFunction0, HostFunction1, HostFunction2, HostFunction3, HyperlightFunction,
    SupportedParameterType, SupportedReturnType,
};
use hyperlight_host::sandbox::config::SandboxConfiguration;
use hyperlight_host::sandbox::Callable;
#[cfg(all(feature = "seccomp", target_os = "linux"))]
use hyperlight_host::sandbox::ExtraAllowedSyscall;
use hyperlight_host::sandbox_state::sandbox::{EvolvableSandbox, Sandbox};
use hyperlight_host::sandbox_state::transition::{MultiUseContextCallback, Noop};
use hyperlight_host::{
    new_error, GuestBinary, MultiUseSandbox, Result, SandboxRunOptions, UninitializedSandbox,
};

use super::metrics::{METRIC_ACTIVE_PROTO_WASM_SANDBOXES, METRIC_TOTAL_PROTO_WASM_SANDBOXES};
use super::sandbox_builder::SandboxBuilder;
use super::wasm_sandbox::WasmSandbox;
use crate::build_info::BuildInfo;
use crate::{HostPrintFn, ReturnType, ReturnValue};

/// A Hyperlight Sandbox with no Wasm run time loaded and no guest module code loaded.
/// This is used to register new host functions that can be called by guest code.
///
/// Once all guest functions have been loaded you can call `load_runtime to load the Wasm runtime and get a `WasmSandbox`.
///
/// With that `WasmSandbox` you can load a Wasm module through the `load_module` method and get a `LoadedWasmSandbox` which can then execute functions defined in the Wasm module.
pub struct ProtoWasmSandbox {
    pub(super) inner: Option<UninitializedSandbox>,
}

impl Sandbox for ProtoWasmSandbox {}

impl Registerable for ProtoWasmSandbox {
    fn register_host_function(
        &mut self,
        hfd: &HostFunctionDefinition,
        hf: HyperlightFunction,
    ) -> Result<()> {
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))
            .and_then(|sb| sb.register_host_function(hfd, hf))
    }
    #[cfg(all(feature = "seccomp", target_os = "linux"))]
    fn register_host_function_with_syscalls(
        &mut self,
        hfd: &HostFunctionDefinition,
        hf: HyperlightFunction,
        eas: Vec<ExtraAllowedSyscall>,
    ) -> Result<()> {
        self.inner
            .as_mut()
            .ok_or(new_error!("inner sandbox was none"))
            .and_then(|sb| sb.register_host_function_with_syscalls(hfd, hf, eas))
    }
}

impl<'a> ProtoWasmSandbox {
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
        opts: Option<SandboxRunOptions>,
        guest_binary: GuestBinary,
        host_print_fn: Option<HostPrintFn<'a>>,
    ) -> Result<Self> {
        BuildInfo::log();
        let inner = UninitializedSandbox::new(guest_binary, cfg, opts, host_print_fn)?;
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
        let multi_use_sandbox: MultiUseSandbox = match self.inner.take() {
            Some(s) => s.evolve(Noop::default())?,
            None => return Err(new_error!("No inner sandbox found.")),
        };

        let func = Box::new(move |call_ctx: &mut MultiUseGuestCallContext| {
            let res = call_ctx.call("InitWasmRuntime", ReturnType::Int, None)?;
            if res != ReturnValue::Int(0) {
                return Err(new_error!(
                    "InitWasmRuntime Failed  with error code {:?}",
                    res
                ));
            }
            Ok(())
        });

        let transition_func = MultiUseContextCallback::from(func);

        let new_sbox: MultiUseSandbox = multi_use_sandbox.evolve(transition_func)?;

        Ok(WasmSandbox::new(new_sbox))
    }

    /// Register the given 0-parameter host function `func` with `self` under
    /// the given `name`. Return `Ok` if the registration succeeded, and a
    /// descriptive `Err` otherwise.
    pub fn register_host_func_0<R: SupportedReturnType<R>>(
        &mut self,
        name: &str,
        func: &dyn HostFunction0<'a, R>,
    ) -> Result<()> {
        match &mut self.inner {
            Some(inner) => func.register(inner, name),
            None => Err(new_error!("inner sandbox was none")),
        }
    }

    /// Register the given 0-parameter host function `func` with `self` along with a vec of syscalls the
    /// function requires and return `Ok` if the registration succeeded, and a descriptive `Err` if
    /// it didn't.
    #[cfg(all(feature = "seccomp", target_os = "linux"))]
    pub fn register_host_func_with_extra_allowed_syscalls_0<R: SupportedReturnType<R>>(
        &mut self,
        name: &str,
        func: &dyn HostFunction0<'a, R>,
        extra_allowed_syscalls: Vec<ExtraAllowedSyscall>,
    ) -> Result<()> {
        match &mut self.inner {
            Some(inner) => {
                func.register_with_extra_allowed_syscalls(inner, name, extra_allowed_syscalls)
            }
            None => Err(new_error!("inner sandbox was none")),
        }
    }

    /// Register the given 1-parameter host function `func` with `self`, and
    /// return `Ok` if the registration succeeded, and a descriptive `Err` if
    /// it didn't.
    pub fn register_host_func_1<
        P1: SupportedParameterType<P1> + Clone + 'a,
        R: SupportedReturnType<R>,
    >(
        &mut self,
        name: &str,
        func: &dyn HostFunction1<'a, P1, R>,
    ) -> Result<()> {
        match &mut self.inner {
            Some(inner) => func.register(inner, name),
            None => Err(new_error!("inner sandbox was none")),
        }
    }

    /// Register the given 1-parameter host function `func` with `self` along with a vec of syscalls the
    /// function requires and return `Ok` if the registration succeeded, and a descriptive `Err` if
    /// it didn't.
    #[cfg(all(feature = "seccomp", target_os = "linux"))]
    pub fn register_host_func_with_extra_allowed_syscalls_1<
        P1: SupportedParameterType<P1> + Clone + 'a,
        R: SupportedReturnType<R>,
    >(
        &mut self,
        name: &str,
        func: &dyn HostFunction1<'a, P1, R>,
        extra_allowed_syscalls: Vec<ExtraAllowedSyscall>,
    ) -> Result<()> {
        match &mut self.inner {
            Some(inner) => {
                func.register_with_extra_allowed_syscalls(inner, name, extra_allowed_syscalls)
            }
            None => Err(new_error!("inner sandbox was none")),
        }
    }

    /// Register the given 2-parameter host function `func` with `self`, and
    /// return `Ok` if the registration succeeded, and a descriptive `Err` if
    /// it didn't.
    pub fn register_host_func_2<
        P1: SupportedParameterType<P1> + Clone + 'a,
        P2: SupportedParameterType<P2> + Clone + 'a,
        R: SupportedReturnType<R>,
    >(
        &mut self,
        name: &str,
        func: &dyn HostFunction2<'a, P1, P2, R>,
    ) -> Result<()> {
        match &mut self.inner {
            Some(inner) => func.register(inner, name),
            None => Err(new_error!("inner sandbox was none")),
        }
    }

    /// Register the given 2-parameter host function `func` with `self` along with a vec of syscalls the
    /// function requires and return `Ok` if the registration succeeded, and a descriptive `Err` if
    /// it didn't.
    #[cfg(all(feature = "seccomp", target_os = "linux"))]
    pub fn register_host_func_with_extra_allowed_syscalls_2<
        P1: SupportedParameterType<P1> + Clone + 'a,
        P2: SupportedParameterType<P2> + Clone + 'a,
        R: SupportedReturnType<R>,
    >(
        &mut self,
        name: &str,
        func: &dyn HostFunction2<'a, P1, P2, R>,
        extra_allowed_syscalls: Vec<ExtraAllowedSyscall>,
    ) -> Result<()> {
        match &mut self.inner {
            Some(inner) => {
                func.register_with_extra_allowed_syscalls(inner, name, extra_allowed_syscalls)
            }
            None => Err(new_error!("inner sandbox was none")),
        }
    }

    /// Register the given 3-parameter host function `func` with `self`, and
    /// return `Ok` if the registration succeeded, and a descriptive `Err` if
    /// it didn't.
    pub fn register_host_func_3<
        P1: SupportedParameterType<P1> + Clone + 'a,
        P2: SupportedParameterType<P2> + Clone + 'a,
        P3: SupportedParameterType<P3> + Clone + 'a,
        R: SupportedReturnType<R>,
    >(
        &mut self,
        name: &str,
        func: &dyn HostFunction3<'a, P1, P2, P3, R>,
    ) -> Result<()> {
        match &mut self.inner {
            Some(inner) => func.register(inner, name),
            None => Err(new_error!("inner sandbox was none")),
        }
    }

    /// Register the given 3-parameter host function `func` with `self` along with a vec of syscalls the
    /// function requires and return `Ok` if the registration succeeded, and a descriptive `Err` if
    /// it didn't.
    #[cfg(all(feature = "seccomp", target_os = "linux"))]
    pub fn register_host_func_with_extra_allowed_syscalls_3<
        P1: SupportedParameterType<P1> + Clone + 'a,
        P2: SupportedParameterType<P2> + Clone + 'a,
        P3: SupportedParameterType<P3> + Clone + 'a,
        R: SupportedReturnType<R>,
    >(
        &mut self,
        name: &str,
        func: &dyn HostFunction3<'a, P1, P2, P3, R>,
        extra_allowed_syscalls: Vec<ExtraAllowedSyscall>,
    ) -> Result<()> {
        match &mut self.inner {
            Some(inner) => {
                func.register_with_extra_allowed_syscalls(inner, name, extra_allowed_syscalls)
            }
            None => Err(new_error!("inner sandbox was none")),
        }
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

// The default configuration is as follows:

// The Hyperlight default Host Print implementation is used.
// The Sandbox will attempt to run in a hypervisor if and will fail if no Hypervisor is available.
// The Sandbox will have  a stack size of 8K and a heap size of 64K
// The Sandbox will have a max execution time of 1000ms and a max wait for cancellation of 100ms
// All other Hyperlight configuration values will be set to their defaults.

// This will result in a memory footprint for the VM backing the Sandbox of approximately 434K

/// Use the `load_runtime` method to
/// load the Wasm runtime and convert the returned `ProtoWasmSandbox`
/// into a `WasmSandbox` that can have a Wasm module loaded returning a `LoadedWasmSandbox` that can be used to call Wasm functions in the guest.
///

impl Default for ProtoWasmSandbox {
    fn default() -> Self {
        SandboxBuilder::new().build().unwrap()
    }
}
