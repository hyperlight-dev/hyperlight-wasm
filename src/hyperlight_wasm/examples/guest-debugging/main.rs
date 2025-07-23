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

use examples_common::get_wasm_module_path;
use hyperlight_host::HyperlightError;
use hyperlight_wasm::{Result, SandboxBuilder};

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
    let tests = [
        (
            "HelloWorld.aot",
            "HelloWorld",
            "Message from Rust Example to Wasm Function".to_string(),
        ),
        // (
        //     "rust_wasm_samples.aot",
        //     "add",
        //     (5i32, 3i32),
        // ),
    ];
    for (idx, case) in tests.iter().enumerate() {
        let (mod_path, fn_name, params_opt) = case;

        let mut sandbox = builder().build()?;

        let wasm_sandbox = match mod_path.starts_with("RunWasm") {
            true => {
                sandbox
                    .register(
                        "GetTimeSinceBootMicrosecond",
                        get_time_since_boot_microsecond,
                    )
                    .unwrap();

                sandbox.load_runtime()?
            }
            false => sandbox.load_runtime()?,
        };

        let mod_path = get_wasm_module_path(mod_path)?;

        // Load a Wasm module into the sandbox
        let mut loaded_wasm_sandbox = wasm_sandbox.load_module(mod_path)?;

        if *fn_name == "Echo" {
            // Call a function in the Wasm module
            let result: String =
                loaded_wasm_sandbox.call_guest_function(fn_name, params_opt.clone())?;
            println!(
                "Result from calling Echo Function in Wasm Module \
                test case {idx}) is: {}",
                result
            );
        } else if *fn_name == "HelloWorld" {
            // Call a function in the Wasm module
            let result: i32 =
                loaded_wasm_sandbox.call_guest_function(fn_name, params_opt.clone())?;

            println!(
                "Result from calling HelloWorld Function in Wasm Module \
            test case {idx}) is: {}",
                result
            );
        }
    }
    Ok(())
}
