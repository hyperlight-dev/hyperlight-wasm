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
use hyperlight_host::HyperlightError;
use hyperlight_wasm::{Result, SandboxBuilder};

fn get_time_since_boot_microsecond() -> Result<i64> {
    let res = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)?
        .as_micros();
    i64::try_from(res).map_err(HyperlightError::IntConversionFailure)
}

fn main() -> Result<()> {
    let tests = [
        (
            "HelloWorld.aot",
            "HelloWorld",
            "Message from Rust Example to Wasm Function".to_string(),
        ),
        (
            "RunWasm.aot",
            "Echo",
            "Message from Rust Example to Wasm Function".to_string(),
        ),
    ];
    for (idx, case) in tests.iter().enumerate() {
        let (mod_path, fn_name, params_opt) = case;

        let mut sandbox = SandboxBuilder::new().build()?;

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
