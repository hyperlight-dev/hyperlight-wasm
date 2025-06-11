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
use hyperlight_wasm::{Result, SandboxBuilder};

fn main() {
    // Create a Wasm Sandbox (this is running in the local Hypervisor)

    let mut proto_wasm_sandbox = SandboxBuilder::new()
        .with_guest_input_buffer_size(0x40000)
        .with_guest_heap_size(0x70000)
        .build()
        .unwrap();

    // Create a host-provided function and register it on the WasmSandbox.
    let host_func = |a: i32| {
        println!("host_func called with {}", a);
        a + 1
    };

    proto_wasm_sandbox
        .register("TestHostFunc", host_func)
        .unwrap();

    let wasm_sandbox = proto_wasm_sandbox.load_runtime().unwrap();

    let mut loaded_wasm_sandbox = {
        let mod_path = get_wasm_module_path("rust_wasm_samples.wasm").unwrap();
        wasm_sandbox.load_module(mod_path)
    }
    .unwrap();

    let result: i32 = loaded_wasm_sandbox
        .call_guest_function("call_host_function", 5i32)
        .unwrap();

    println!("got result: {:?} from the host function!", result);
}
