/*
Copyright 2025 The Hyperlight Authors.

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

//! Example demonstrating how to enable guest debugging in a Wasm sandbox
//!
//! The prerequisite for this example is to have a Wasm modules compiled with debug
//! symbols.
//!
//! To compile the necessary Wasm modules with debug symbols, you can use the following commands:
//! ```sh
//! # for C modules
//! just build-wasm-examples debug gdb
//! # for Rust modules
//! just build-rust-wasm-samples debug gdb
//! ```
//!
//! # Running the Example
//! To run the example with GDB support, execute the following command:
//! ```sh
//! cargo run --example guest-debugging --features gdb
//! ```
//!
//! This will start the example and enable debugging on port 8080.
//! You can then connect to the running Wasm guest using GDB or LLDB.
//! Check docs/wasm-modules-debugging.md for more details on how to connect the debugger.
//!
//! NOTE: The guest binary containing the wasm runtime needs to be used initially to attach
//! with the debugger. The path to the binary is written to the terminal when the example
//! is built.

use examples_common::get_wasm_module_path;
use hyperlight_host::HyperlightError;
use hyperlight_wasm::{LoadedWasmSandbox, Result, SandboxBuilder};

fn get_time_since_boot_microsecond() -> Result<i64> {
    let res = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)?
        .as_micros();
    i64::try_from(res).map_err(HyperlightError::IntConversionFailure)
}

fn builder() -> SandboxBuilder {
    #[cfg(gdb)]
    {
        SandboxBuilder::new()
            .with_guest_input_buffer_size(2 * 1024 * 1024) // 2 MiB
            .with_guest_heap_size(10 * 1024 * 1024) // 4 MiB
            .with_debugging_enabled(8080) // debugging on port 8080
    }
    #[cfg(not(gdb))]
    SandboxBuilder::new()
}

fn main() -> Result<()> {
    type TestFn = fn(&mut LoadedWasmSandbox, &str, &str) -> Result<i32>;
    let tests: &[(&str, &str, TestFn)] = &[
        ("HelloWorld.aot", "HelloWorld", |sb, func_name, module| {
            println!("Calling {} into \"{}\" module", func_name, module);
            sb.call_guest_function(
                func_name,
                "Message from C Example to Wasm Function".to_string(),
            )
        }),
        ("rust_wasm_samples.aot", "add", |sb, func_name, module| {
            println!("Calling {} into \"{}\" module", func_name, module);
            let res = sb.call_guest_function(func_name, (5i32, 3i32));
            if let Ok(sum) = res {
                println!("add(5, 3) = {}", sum);
            }
            res
        }),
        (
            "rust_wasm_samples.aot",
            "call_host_function",
            |sb, func_name, module| {
                println!("Calling {} into \"{}\" module", func_name, module);
                let res = sb.call_guest_function(func_name, 5i32);
                if let Ok(val) = res {
                    println!("call_host_function(5) = {}", val);
                }
                res
            },
        ),
    ];
    let mut sandbox = builder().build()?;
    let host_func = |a: i32| {
        println!("host_func called with {}", a);
        a + 1
    };

    sandbox.register("TestHostFunc", host_func)?;
    sandbox.register(
        "GetTimeSinceBootMicrosecond",
        get_time_since_boot_microsecond,
    )?;
    let mut wasm_sandbox = Some(sandbox.load_runtime()?);

    for (module, fn_name, func) in tests.iter() {
        let mod_path = get_wasm_module_path(module)?;

        // Load a Wasm module into the sandbox
        let mut loaded_wasm_sandbox = wasm_sandbox.take().unwrap().load_module(mod_path)?;

        let _ = func(&mut loaded_wasm_sandbox, fn_name, module)?;

        wasm_sandbox = Some(loaded_wasm_sandbox.unload_module()?);
    }
    Ok(())
}
