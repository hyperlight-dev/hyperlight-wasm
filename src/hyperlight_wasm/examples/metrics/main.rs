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
use hyperlight_wasm::{ParameterValue, Result, ReturnType, SandboxBuilder};

fn main() -> Result<()> {
    // Install prometheus metrics exporter.
    // We only install the metrics recorder here, but you can also use the
    // `metrics_exporter_prometheus::PrometheusBuilder::new().install()` method
    // to install a HTTP listener that serves the metrics.
    let prometheus_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus exporter");

    for _ in 0..10 {
        let host_func = Arc::new(Mutex::new(|a: i32| -> Result<i32> {
            println!("host_func called with {}", a);
            Ok(a + 1)
        }));

        let mut wasm_sandbox = SandboxBuilder::new()
            .with_guest_function_call_max_cancel_wait_millis(100)
            .with_guest_input_buffer_size(1000000)
            .build()?;

        wasm_sandbox.register_host_func_1("TestHostFunc", &host_func)?;

        let wasm_sandbox = wasm_sandbox.load_runtime()?;

        let mut loaded_wasm_sandbox =
            wasm_sandbox.load_module(get_wasm_module_path("rust_wasm_samples.wasm")?)?;

        loaded_wasm_sandbox
            .call_guest_function(
                "add",
                Some(vec![ParameterValue::Int(5), ParameterValue::Int(10)]),
                ReturnType::Int,
            )
            .unwrap();
    }

    // Render out the metrics in prometheus exposition format.
    // At this point, we should have created 10 of each sandbox, but 0 would be active
    // since they were dropped in above for-loop
    let payload = prometheus_handle.render();
    println!("Prometheus metrics:\n{}", payload);

    Ok(())
}
