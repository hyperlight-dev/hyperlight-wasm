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

//! This example demonstrates how to:
//! 1. Get an interrupt handle for a sandbox
//! 2. Interrupt long-running guest code from another thread
//! 3. Detect when a sandbox is poisoned
//! 4. Recover a poisoned sandbox using `restore()` or `unload_module()`

use std::thread;
use std::time::Duration;

use examples_common::get_wasm_module_path;
use hyperlight_wasm::{HyperlightError, Result, SandboxBuilder};

fn get_time_since_boot_microsecond() -> Result<i64> {
    let res = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)?
        .as_micros();
    i64::try_from(res).map_err(HyperlightError::IntConversionFailure)
}

fn main() -> Result<()> {
    println!("=== Hyperlight-Wasm Interruption Example ===\n");

    // Build a sandbox and register host functions
    let mut sandbox = SandboxBuilder::new().build()?;
    sandbox.register(
        "GetTimeSinceBootMicrosecond",
        get_time_since_boot_microsecond,
    )?;

    let wasm_sandbox = sandbox.load_runtime()?;
    let mod_path = get_wasm_module_path("RunWasm.aot")?;
    let mut loaded = wasm_sandbox.load_module(mod_path)?;

    println!("1. Sandbox created and module loaded");
    assert!(!loaded.is_poisoned()?);
    println!("   is_poisoned: {}", loaded.is_poisoned()?);

    // Take a snapshot before we do anything
    let snapshot = loaded.snapshot()?;
    println!("2. Snapshot taken for later recovery\n");

    // Get an interrupt handle - this can be sent to another thread
    let interrupt = loaded.interrupt_handle()?;
    println!("3. Interrupt handle obtained\n");

    // Spawn a thread that will interrupt the guest after 1 second
    println!("4. Starting long-running guest function...");
    println!("   (A background thread will interrupt it after 1 second)\n");

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        println!("   [Background thread] Calling interrupt.kill()...");
        interrupt.kill();
    });

    // Call a long-running guest function that will be interrupted
    let result = loaded.call_guest_function::<i32>("KeepCPUBusy", 100000i32);

    match result {
        Ok(_) => panic!("   Guest function completed (unexpected!)"),
        Err(HyperlightError::ExecutionCanceledByHost()) => {
            println!("   Guest function was interrupted (ExecutionCanceledByHost)");
        }
        Err(e) => panic!("   Unexpected error: {:?}", e),
    }

    println!("\n5. Checking sandbox state after interruption:");
    println!("   is_poisoned: {}", loaded.is_poisoned()?);

    // Demonstrate that calling a poisoned sandbox fails
    println!("\n6. Attempting to call guest function on poisoned sandbox...");
    let result = loaded.call_guest_function::<i32>("CalcFib", 10i32);

    match result {
        Ok(_) => panic!("   Call succeeded (unexpected!)"),
        Err(HyperlightError::PoisonedSandbox) => {
            println!("   Call failed with PoisonedSandbox error (expected)");
        }
        Err(e) => panic!("   Unexpected error: {:?}", e),
    }

    // Recovery option 1: Use restore() to recover the sandbox
    println!("\n7. Recovering sandbox using restore()...");
    loaded.restore(snapshot)?;
    assert!(!loaded.is_poisoned()?);
    println!("   is_poisoned after restore: {}", loaded.is_poisoned()?);

    // Now we can call guest functions again
    println!("\n8. Calling guest function after recovery...");
    let result: i32 = loaded.call_guest_function("CalcFib", 10i32)?;
    println!("   CalcFib(10) returned: {} (expected 55)", result);

    // Demonstrate recovery option 2: unload_module
    println!("\n9. Demonstrating unload_module recovery...");

    // First, poison the sandbox again
    let interrupt = loaded.interrupt_handle()?;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        interrupt.kill();
    });
    let _ = loaded.call_guest_function::<i32>("KeepCPUBusy", 100000i32);

    assert!(loaded.is_poisoned()?);
    println!("   Sandbox poisoned again {}", loaded.is_poisoned()?);

    // unload_module() will recover the sandbox
    let wasm_sandbox = loaded.unload_module()?;
    println!("   Module unloaded (this calls restore internally)");

    // Load a different module and continue
    let hello_path = get_wasm_module_path("HelloWorld.aot")?;
    let mut new_loaded = wasm_sandbox.load_module(hello_path)?;
    assert!(!new_loaded.is_poisoned()?);
    println!(
        "   New module loaded, is_poisoned: {}",
        new_loaded.is_poisoned()?
    );

    let result: i32 =
        new_loaded.call_guest_function("HelloWorld", "Recovery successful!".to_string())?;

    println!("   HelloWorld returned: {}", result);

    println!("\n=== Example Complete ===");
    Ok(())
}
