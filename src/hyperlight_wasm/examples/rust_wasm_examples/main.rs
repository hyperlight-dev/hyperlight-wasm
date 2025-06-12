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

use std::sync::{Arc, Mutex};

use examples_common::get_wasm_module_path;
use hyperlight_wasm::{ParameterValue, Result, ReturnType, ReturnValue, SandboxBuilder};

fn main() -> Result<()> {
    let tests = [
        ("hello_world", None),
        (
            "add",
            Some(vec![ParameterValue::Int(5), ParameterValue::Int(3)]),
        ),
        ("call_host_function", Some(vec![ParameterValue::Int(5)])),
        ("simd_test", None),
    ];

    for (idx, case) in tests.iter().enumerate() {
        let (fn_name, params_opt) = case;
        let host_func = Arc::new(Mutex::new(|a: i32| -> Result<i32> {
            println!("host_func called with {}", a);
            Ok(a + 1)
        }));

        let mut proto_wasm_sandbox = SandboxBuilder::new()
            .with_guest_input_buffer_size(256 * 1024)
            .with_guest_heap_size(768 * 1024)
            .build()?;

        proto_wasm_sandbox.register_host_func_1("TestHostFunc", &host_func)?;

        let wasm_sandbox = proto_wasm_sandbox.load_runtime()?;
        let mod_path = get_wasm_module_path("rust_wasm_samples.wasm")?;

        // Load the Wasm module into the sandbox
        let mut loaded_wasm_sandbox = wasm_sandbox.load_module(mod_path)?;

        // Call a function in the Wasm module
        let ReturnValue::Int(result) = loaded_wasm_sandbox.call_guest_function(
            fn_name,
            params_opt.clone(),
            ReturnType::Int,
        )?
        else {
            panic!("Failed to get result from call_guest_function")
        };

        println!("test case {idx} fn_name: {fn_name}\nresult: {}", result)
    }
    Ok(())
}
