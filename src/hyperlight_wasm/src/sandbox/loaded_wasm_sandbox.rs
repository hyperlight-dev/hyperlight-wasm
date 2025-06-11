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

use hyperlight_host::func::{ParameterTuple, SupportedReturnType};
use hyperlight_host::sandbox::Callable;
use hyperlight_host::sandbox_state::sandbox::{DevolvableSandbox, Sandbox};
use hyperlight_host::sandbox_state::transition::Noop;
use hyperlight_host::{MultiUseSandbox, Result, log_then_return};

use super::metrics::METRIC_TOTAL_LOADED_WASM_SANDBOXES;
use super::wasm_sandbox::WasmSandbox;
use crate::sandbox::metrics::{METRIC_ACTIVE_LOADED_WASM_SANDBOXES, METRIC_SANDBOX_UNLOADS};

/// A sandbox that has both a Wasm engine and an arbitrary Wasm module
/// loaded into memory.
///
/// `LoadedWasmSandbox`es are ready to execute
/// guest code and can execute a guest call, with `call_guest_function`,
/// multiple times. Each call to `call_guest_function` executes in the same
/// memory context. If you want to "reset" the memory context, create
/// a new `LoadedWasmSandbox` -- either from another `WasmSandbox` or by
/// calling `my_loaded_wasm_sandbox.devolve()?.evolve()?`
#[derive(Debug)]
pub struct LoadedWasmSandbox {
    // inner is an Option<MultiUseSandbox> as we need to take ownership of it
    // We implement drop on the LoadedWasmSandbox to decrement the count of Sandboxes when it is dropped
    // because of this we cannot implement drop without making inner an Option (alternatively we could make MultiUseSandbox Copy but that would introduce other issues)
    inner: Option<MultiUseSandbox>,
}

impl Sandbox for LoadedWasmSandbox {}

impl LoadedWasmSandbox {
    /// Call the function in the guest with the name `fn_name`, passing
    /// parameters `params`.
    ///
    /// On success, return an `Ok` with the return
    /// value and a new copy of `Self` suitable for further use. On failure,
    /// return an appropriate `Err`.
    pub fn call_guest_function<Output: SupportedReturnType>(
        &mut self,
        fn_name: &str,
        params: impl ParameterTuple,
    ) -> Result<Output> {
        match &mut self.inner {
            Some(inner) => inner.call_guest_function_by_name(fn_name, params),
            None => log_then_return!("No inner MultiUseSandbox to call_guest_function"),
        }
    }
    /// unload the wasm module and return a `WasmSandbox` that can be used to load another module
    pub fn unload_module(self) -> Result<WasmSandbox> {
        self.devolve(Noop::default()).inspect(|_| {
            metrics::counter!(METRIC_SANDBOX_UNLOADS).increment(1);
        })
    }

    pub(super) fn new(inner: MultiUseSandbox) -> Result<LoadedWasmSandbox> {
        metrics::gauge!(METRIC_ACTIVE_LOADED_WASM_SANDBOXES).increment(1);
        metrics::counter!(METRIC_TOTAL_LOADED_WASM_SANDBOXES).increment(1);
        Ok(LoadedWasmSandbox { inner: Some(inner) })
    }
}

impl Callable for LoadedWasmSandbox {
    fn call<Output: SupportedReturnType>(
        &mut self,
        func_name: &str,
        args: impl ParameterTuple,
    ) -> Result<Output> {
        self.call_guest_function(func_name, args)
    }
}

/// Capability to transform a `LoadedWasmSandbox` back down to a
/// `WasmSandbox`
impl DevolvableSandbox<LoadedWasmSandbox, WasmSandbox, Noop<LoadedWasmSandbox, WasmSandbox>>
    for LoadedWasmSandbox
{
    fn devolve(mut self, _: Noop<LoadedWasmSandbox, WasmSandbox>) -> Result<WasmSandbox> {
        let new_inner: MultiUseSandbox = match self.inner.take() {
            Some(inner) => inner.devolve(Noop::default())?,
            None => log_then_return!("No inner MultiUseSandbox to devolve"),
        };
        Ok(WasmSandbox::new(new_inner))
    }
}

impl Drop for LoadedWasmSandbox {
    fn drop(&mut self) {
        metrics::gauge!(METRIC_ACTIVE_LOADED_WASM_SANDBOXES).decrement(1);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use crossbeam_queue::ArrayQueue;
    use examples_common::get_wasm_module_path;
    use hyperlight_host::{HyperlightError, new_error};

    use super::{LoadedWasmSandbox, WasmSandbox};
    use crate::Result;
    use crate::sandbox::proto_wasm_sandbox::ProtoWasmSandbox;
    use crate::sandbox::sandbox_builder::SandboxBuilder;

    fn get_time_since_boot_microsecond() -> Result<i64> {
        let res = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)?
            .as_micros();
        i64::try_from(res).map_err(HyperlightError::IntConversionFailure)
    }

    // Ensure that we can use a sandbox multiple times to call guest functions and that we dont run out of memory or have any other issues

    #[test]
    fn test_call_guest_functions_with_default_config_multiple_times() {
        for ext in [".aot", ".wasm"].iter() {
            let mut sandbox = ProtoWasmSandbox::default();

            sandbox
                .register(
                    "GetTimeSinceBootMicrosecond",
                    get_time_since_boot_microsecond,
                )
                .unwrap();

            let wasm_sandbox = sandbox.load_runtime().unwrap();
            let loaded_wasm_sandbox: LoadedWasmSandbox = {
                let name = format!("RunWasm{}", ext);
                let mod_path = get_wasm_module_path(&name).unwrap();
                wasm_sandbox.load_module(mod_path)
            }
            .unwrap();

            call_funcs(loaded_wasm_sandbox, 1000);
        }
    }

    #[test]
    fn test_sandbox_use_on_different_threads() {
        let wasm_sandbox_queue = Arc::new(ArrayQueue::<WasmSandbox>::new(10));
        let loaded_wasm_sandbox_queue = Arc::new(ArrayQueue::<LoadedWasmSandbox>::new(10));

        // Create a queue of WasmSandbox instances
        for i in 0..10 {
            println!("Creating WasmSandbox instance {}", i);
            let mut sandbox = ProtoWasmSandbox::default();

            sandbox
                .register(
                    "GetTimeSinceBootMicrosecond",
                    get_time_since_boot_microsecond,
                )
                .unwrap();

            let wasm_sandbox = sandbox.load_runtime().unwrap();
            wasm_sandbox_queue.push(wasm_sandbox).unwrap();
            println!("Pushed WasmSandbox instance {}", i);
        }

        // Get the WasmSandbox instances from the queue and load the module on a new thread
        // then call a function and push the LoadedWasmSandbox instance to the loaded_wasm_sandbox_queue
        let thread_handles: Vec<_> = (0..10)
            .map(|i| {
                let wq = wasm_sandbox_queue.clone();
                let lwq = loaded_wasm_sandbox_queue.clone();

                thread::spawn(move || {
                    println!("Loading module on thread {}", i);
                    let wasm_sandbox = wq.pop().unwrap();
                    let loaded_wasm_sandbox: LoadedWasmSandbox = {
                        let mod_path = get_wasm_module_path("RunWasm.aot").unwrap();
                        wasm_sandbox.load_module(mod_path)
                    }
                    .unwrap();
                    println!("Calling function on thread {}", i);
                    let lws = call_funcs(loaded_wasm_sandbox, 1);
                    lwq.push(lws).unwrap();
                    println!("Pushed LoadedWasmSandbox instance to queue on thread {}", i)
                })
            })
            .collect::<Vec<_>>();

        for handle in thread_handles {
            handle.join().unwrap();
        }

        // Get the LoadedWasmSandbox instances from the queue and call a function on a new thread, then unload the module and
        // push the WasmSandbox instance back to the wasm_sandbox_queue

        let thread_handles: Vec<_> = (0..10)
            .map(|i| {
                let wq = wasm_sandbox_queue.clone();
                let lwq = loaded_wasm_sandbox_queue.clone();

                thread::spawn(move || {
                    println!("Popping sandbox on thread {}", i);
                    let loaded_wasm_sandbox = lwq.pop().unwrap();
                    println!("Calling funcs on thread {}", i);
                    let lws = call_funcs(loaded_wasm_sandbox, 1);
                    println!("Unloading module on thread {}", i);
                    let ws = lws.unload_module().unwrap();
                    println!("Pusing WasmSandbox on thread {}", i);
                    wq.push(ws).unwrap();
                })
            })
            .collect::<Vec<_>>();

        for handle in thread_handles {
            handle.join().unwrap();
        }

        // Now get the sandbox back from the queue and load the module and call a function
        // this time we will load the .wasm version of the module rather than the .aot version

        let thread_handles: Vec<_> = (0..10)
            .map(|i| {
                let wq = wasm_sandbox_queue.clone();

                thread::spawn(move || {
                    println!("Popping WasmSandbox on thread {}", i);
                    let wasm_sandbox = wq.pop().unwrap();
                    println!("Loading module on thread {}", i);
                    let loaded_wasm_sandbox: LoadedWasmSandbox = {
                        let mod_path = get_wasm_module_path("RunWasm.wasm").unwrap();
                        wasm_sandbox.load_module(mod_path)
                    }
                    .unwrap();
                    println!("Calling function on thread {}", i);
                    call_funcs(loaded_wasm_sandbox, 1);
                })
            })
            .collect::<Vec<_>>();

        for handle in thread_handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_call_guest_functions_with_custom_config_multiple_times() {
        for ext in [".aot", ".wasm"].iter() {
            let mut sandbox = SandboxBuilder::new()
                .with_guest_stack_size(32 * 1024)
                .with_guest_heap_size(128 * 1024)
                .build()
                .unwrap();

            sandbox
                .register(
                    "GetTimeSinceBootMicrosecond",
                    get_time_since_boot_microsecond,
                )
                .unwrap();

            let wasm_sandbox = sandbox.load_runtime().unwrap();

            let loaded_wasm_sandbox: LoadedWasmSandbox = {
                let name = format!("RunWasm{}", ext);
                let mod_path = get_wasm_module_path(&name).unwrap();
                wasm_sandbox.load_module(mod_path)
            }
            .unwrap();

            call_funcs(loaded_wasm_sandbox, 1000);
        }
    }

    #[test]
    fn test_call_host_func_with_vecbytes() {
        let host_func = |b: Vec<u8>, l: i32| {
            // get the C String from the vec of bytes

            let s = std::str::from_utf8(&b).unwrap();
            println!("Host function received buffer: {}", s);

            // check that s is the expected value if not return an error
            if s != "Hello World!" {
                return Err(new_error!("Unexpected value in buffer {}", s));
            }

            if l != 12 {
                return Err(new_error!("Unexpected length of buffer {}", l));
            }
            Ok(0i32)
        };

        for ext in [".aot", ".wasm"].iter() {
            let mut proto_wasm_sandbox = SandboxBuilder::new().build().unwrap();

            proto_wasm_sandbox
                .register("HostFuncWithBufferAndLength", host_func)
                .unwrap();

            let wasm_sandbox = proto_wasm_sandbox.load_runtime().unwrap();

            let mut loaded_wasm_sandbox: LoadedWasmSandbox = {
                let name = format!("HostFunction{}", ext);
                let mod_path = get_wasm_module_path(&name).unwrap();
                wasm_sandbox.load_module(mod_path)
            }
            .unwrap();

            // Call a guest function that calls a host function that takes a buffer and a length

            let r: i32 = loaded_wasm_sandbox
                .call_guest_function("PassBufferAndLengthToHost", ())
                .unwrap();

            assert_eq!(r, 0);
        }
    }

    fn call_funcs(
        mut loaded_wasm_sandbox: LoadedWasmSandbox,
        iterations: i32,
    ) -> LoadedWasmSandbox {
        // Call a guest function that returns an int

        for i in 0..iterations {
            let result: i32 = loaded_wasm_sandbox
                .call_guest_function("CalcFib", 4i32)
                .unwrap();

            println!(
                "Got result: {:?} from the host function! iteration {}",
                result, i,
            );
        }

        // Call a guest function that returns a string

        for i in 0..iterations {
            let result: String = loaded_wasm_sandbox
                .call_guest_function(
                    "Echo",
                    "Message from Rust Example to Wasm Function".to_string(),
                )
                .unwrap();

            println!(
                "Got result: {:?} from the host function! iteration {}",
                result, i,
            );
        }

        for i in 0..iterations {
            let result: String = loaded_wasm_sandbox
                .call_guest_function(
                    "ToUpper",
                    "Message from Rust Example to WASM Function".to_string(),
                )
                .unwrap();

            println!(
                "Got result: {:?} from the host function! iteration {}",
                result, i,
            );

            assert_eq!(
                result,
                "MESSAGE FROM RUST EXAMPLE TO WASM FUNCTION".to_string()
            );
        }

        // Call a guest function that returns a size prefixed buffer

        for i in 0..iterations {
            let result: Vec<u8> = loaded_wasm_sandbox
                .call_guest_function("ReceiveByteArray", (vec![0x01, 0x02, 0x03], 3i32))
                .unwrap();

            println!(
                "Got result: {:?} from the host function! iteration {}",
                result, i,
            );
        }

        // Call a guest function that Prints a string using HostPrint Host function

        for i in 0..iterations {
            loaded_wasm_sandbox
                .call_guest_function::<()>(
                    "Print",
                    "Message from Rust Example to Wasm Function\n".to_string(),
                )
                .unwrap();

            println!("Called the host function! iteration {}", i,);
        }

        // Call a guest function that calls prints a string constant using printf

        for i in 0..iterations {
            loaded_wasm_sandbox
                .call_guest_function::<()>("PrintHelloWorld", ())
                .unwrap();

            println!("Called the host function! iteration {}", i,);
        }

        loaded_wasm_sandbox
    }
}
