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
use std::thread::{spawn, JoinHandle};

use examples_common::get_wasm_module_path;
use hyperlight_host::HyperlightError;
use hyperlight_wasm::{
    set_metrics_registry, LoadedWasmSandbox, ParameterValue, Result, ReturnType, ReturnValue,
    SandboxBuilder,
};
use lazy_static::lazy_static;
use prometheus::Registry;

lazy_static! {
    static ref HOST_REGISTRY: Registry = Registry::new();
}
fn fn_writer(msg: String) -> Result<i32> {
    Ok(msg.len() as i32)
}

fn get_time_since_boot_microsecond() -> Result<i64> {
    let res = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)?
        .as_micros();
    i64::try_from(res).map_err(HyperlightError::IntConversionFailure)
}

fn main() -> Result<()> {
    // If this is not called then the default registry `prometheus::default_registry` will be used.
    set_metrics_registry(&HOST_REGISTRY)?;

    let mut join_handles: Vec<JoinHandle<Result<()>>> = vec![];

    for _ in 0..20 {
        let writer_func = Arc::new(Mutex::new(fn_writer));
        let get_time_since_boot_microsecond_func =
            Arc::new(Mutex::new(get_time_since_boot_microsecond));
        let handle = spawn(move || -> Result<()> {
            // Create a new WASM sandbox.
            let mut sandbox = SandboxBuilder::new()
                .with_host_print_fn(&writer_func)
                .build()?;
            sandbox
                .register_host_func_0(
                    "GetTimeSinceBootMicrosecond",
                    &get_time_since_boot_microsecond_func,
                )
                .unwrap();

            let mut wasm_sandbox = sandbox.load_runtime()?;

            // Load a WASM Module into the sandbox.

            let mut loaded_wasm_sandbox =
                wasm_sandbox.load_module(get_wasm_module_path("RunWasm.aot")?)?;

            // Call guest functions 50 times to generate some metrics.

            loaded_wasm_sandbox = call_funcs(loaded_wasm_sandbox, 50);

            wasm_sandbox = loaded_wasm_sandbox.unload_module()?;

            loaded_wasm_sandbox =
                wasm_sandbox.load_module(get_wasm_module_path("RunWasm.wasm")?)?;

            // Call guest functions 50 times to generate some more metrics.

            call_funcs(loaded_wasm_sandbox, 50);

            Ok(())
        });

        join_handles.push(handle);
    }

    // Generate some metrics for cancelled calls.

    for i in 0..10 {
        let wasm_sandbox = SandboxBuilder::new()
            .with_guest_function_call_max_cancel_wait_millis(100)
            .build()?
            .load_runtime()?;

        let mut loaded_wasm_sandbox =
            wasm_sandbox.load_module(get_wasm_module_path("RunWasm.aot")?)?;

        match loaded_wasm_sandbox.call_guest_function(
            "CalcFib",
            Some(vec![ParameterValue::Int(400)]),
            ReturnType::Int,
        ) {
            Ok(ReturnValue::Int(result)) => {
                println!(
                    "Got result: {:?} from the host function! iteration {}",
                    result, i,
                );
            }
            Ok(_) => {
                println!("Failed to get result from CalcFib iteration {}", i)
            }
            Err(e) => {
                println!("Error calling CalcFib: {:?}", e)
            }
        }
    }

    for join_handle in join_handles {
        let result = join_handle.join();
        assert!(result.is_ok());
    }

    get_metrics();

    Ok(())
}

fn get_metrics() {
    // Get the metrics from the registry.

    let metrics = HOST_REGISTRY.gather();

    // Print the metrics.

    print!("\nMETRICS:\n");

    for metric in metrics.iter() {
        match metric.get_field_type() {
            prometheus::proto::MetricType::COUNTER => {
                println!("Counter: {:?}", metric.get_help());
                metric.get_metric().iter().for_each(|metric| {
                    let pair = metric.get_label();
                    for pair in pair.iter() {
                        println!("Label: {:?} Name: {:?}", pair.get_name(), pair.get_value());
                    }
                    println!("Value: {:?}", metric.get_counter().get_value());
                });
            }
            prometheus::proto::MetricType::GAUGE => {
                println!("Gauge: {:?}", metric.get_help());
                metric.get_metric().iter().for_each(|metric| {
                    let pair = metric.get_label();
                    for pair in pair.iter() {
                        println!("Label: {:?} Name: {:?}", pair.get_name(), pair.get_value());
                    }
                    println!("Value: {:?}", metric.get_gauge().get_value());
                });
            }
            prometheus::proto::MetricType::UNTYPED => {
                println!("Metric: {:?}", metric.get_help());
            }
            prometheus::proto::MetricType::HISTOGRAM => {
                println!("Histogram: {:?}", metric.get_help());
                for metric in metric.get_metric() {
                    let pair = metric.get_label();
                    for pair in pair.iter() {
                        println!("Label: {:?} Name: {:?}", pair.get_name(), pair.get_value());
                    }
                    let count = metric.get_histogram().get_sample_count();
                    println!("Number of observations: {:?}", count);
                    let sm = metric.get_histogram().get_sample_sum();
                    println!("Sum of observations: {:?}", sm);
                    metric
                        .get_histogram()
                        .get_bucket()
                        .iter()
                        .for_each(|bucket| {
                            println!(
                                "Bucket: {:?} Count: {:?}",
                                bucket.get_upper_bound(),
                                bucket.get_cumulative_count()
                            )
                        });
                }
            }
            prometheus::proto::MetricType::SUMMARY => {
                println!("Summary: {:?}", metric.get_help());
            }
        }
    }
}

fn call_funcs(mut loaded_wasm_sandbox: LoadedWasmSandbox, iterations: i32) -> LoadedWasmSandbox {
    // Call a guest function that returns an int

    for i in 0..iterations {
        let ReturnValue::Int(result) = loaded_wasm_sandbox
            .call_guest_function(
                "CalcFib",
                Some(vec![ParameterValue::Int(4)]),
                ReturnType::Int,
            )
            .unwrap()
        else {
            panic!(
                "Failed to get result from call_guest_function  iteration {}",
                i
            )
        };

        println!(
            "Got result: {:?} from the host function! iteration {}",
            result, i,
        );
    }

    // Call a guest function that returns a string

    for i in 0..iterations {
        let ReturnValue::String(result) = loaded_wasm_sandbox
            .call_guest_function(
                "Echo",
                Some(vec![ParameterValue::String(
                    "Message from Rust Example to Wasm Function".to_string(),
                )]),
                ReturnType::String,
            )
            .unwrap()
        else {
            panic!(
                "Failed to get result from call_guest_function  iteration {}",
                i
            )
        };

        println!(
            "Got result: {:?} from the host function! iteration {}",
            result, i,
        );
    }

    for i in 0..iterations {
        let ReturnValue::String(result) = loaded_wasm_sandbox
            .call_guest_function(
                "ToUpper",
                Some(vec![ParameterValue::String(
                    "Message from Rust Example to WASM Function".to_string(),
                )]),
                ReturnType::String,
            )
            .unwrap()
        else {
            panic!(
                "Failed to get result from call_guest_function  iteration {}",
                i
            )
        };

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
        let ReturnValue::VecBytes(result) = loaded_wasm_sandbox
            .call_guest_function(
                "ReceiveByteArray",
                Some(vec![
                    ParameterValue::VecBytes(vec![0x01, 0x02, 0x03]),
                    ParameterValue::Int(3),
                ]),
                ReturnType::VecBytes,
            )
            .unwrap()
        else {
            panic!(
                "Failed to get result from call_guest_function  iteration {}",
                i
            )
        };

        println!(
            "Got result: {:?} from the host function! iteration {}",
            result, i,
        );
    }

    // Call a guest function that Prints a string using HostPrint Host function

    for i in 0..iterations {
        let ReturnValue::Void = loaded_wasm_sandbox
            .call_guest_function(
                "Print",
                Some(vec![ParameterValue::String(
                    "Message from Rust Example to Wasm Function\n".to_string(),
                )]),
                ReturnType::Void,
            )
            .unwrap()
        else {
            panic!(
                "Failed to get result from call_guest_function  iteration {}",
                i
            )
        };

        println!("Called the host function! iteration {}", i,);
    }

    // Call a guest function that calls prints a string constant using printf

    for i in 0..iterations {
        let ReturnValue::Void = loaded_wasm_sandbox
            .call_guest_function("PrintHelloWorld", None, ReturnType::Void)
            .unwrap()
        else {
            panic!(
                "Failed to get result from call_guest_function iteration {}",
                i
            )
        };

        println!("Called the host function! iteration {}", i,);
    }

    loaded_wasm_sandbox
}
