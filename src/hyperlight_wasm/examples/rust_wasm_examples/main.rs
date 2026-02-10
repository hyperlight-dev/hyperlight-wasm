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

use examples_common::get_wasm_module_path;
use hyperlight_wasm::{LoadedWasmSandbox, Result, SandboxBuilder};

fn main() -> Result<()> {
    type TestFn = fn(&mut LoadedWasmSandbox) -> Result<i32>;
    let tests: &[(String, TestFn)] = &[
        ("hello_world".to_string(), |sb| {
            sb.call_guest_function("hello_world", ())
        }),
        ("add".to_string(), |sb| {
            sb.call_guest_function("add", (5i32, 3i32))
        }),
        ("call_host_function".to_string(), |sb| {
            sb.call_guest_function("call_host_function", 5i32)
        }),
    ];

    for (idx, case) in tests.iter().enumerate() {
        let (fn_name, func) = case;
        let host_func = |a: i32| {
            println!("host_func called with {}", a);
            a + 1
        };

        let mut proto_wasm_sandbox = SandboxBuilder::new()
            .with_guest_input_buffer_size(256 * 1024)
            .with_guest_heap_size(1280 * 1024)
            .build()?;

        proto_wasm_sandbox.register("TestHostFunc", host_func)?;

        let wasm_sandbox = proto_wasm_sandbox.load_runtime()?;
        let mod_path = get_wasm_module_path("rust_wasm_samples.aot")?;

        // Load the Wasm module into the sandbox
        let mut loaded_wasm_sandbox = wasm_sandbox.load_module(mod_path)?;

        // Call a function in the Wasm module
        let result: i32 = func(&mut loaded_wasm_sandbox)?;

        println!("test case {idx} fn_name: {fn_name}\nresult: {}", result)
    }
    Ok(())
}
