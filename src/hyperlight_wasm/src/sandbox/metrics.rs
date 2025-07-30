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

/*!
This module contains the definitions and implementations of the metrics used by the sandbox module
*/

// Gauges, active sandboxes
pub(crate) static METRIC_ACTIVE_PROTO_WASM_SANDBOXES: &str = "active_proto_wasm_sandboxes";
pub(crate) static METRIC_ACTIVE_WASM_SANDBOXES: &str = "active_wasm_sandboxes";
pub(crate) static METRIC_ACTIVE_LOADED_WASM_SANDBOXES: &str = "active_loaded_wasm_sandboxes";

// Counters, total sandboxes created during the lifetime of the process
pub(crate) static METRIC_TOTAL_PROTO_WASM_SANDBOXES: &str = "proto_wasm_sandboxes_total";
pub(crate) static METRIC_TOTAL_WASM_SANDBOXES: &str = "wasm_sandboxes_total";
pub(crate) static METRIC_TOTAL_LOADED_WASM_SANDBOXES: &str = "loaded_wasm_sandboxes_total";

// Counters, total number of times loaded sandboxes have been loaded/unloaded during the lifetime of the process
pub(crate) static METRIC_SANDBOX_LOADS: &str = "sandbox_loads_total";
pub(crate) static METRIC_SANDBOX_UNLOADS: &str = "sandbox_unloads_total";

#[cfg(test)]
mod tests {
    use examples_common::get_wasm_module_path;
    use hyperlight_host::HyperlightError;

    use crate::{LoadedWasmSandbox, ProtoWasmSandbox, Result};

    fn get_time_since_boot_microsecond() -> Result<i64> {
        let res = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)?
            .as_micros();
        i64::try_from(res).map_err(HyperlightError::IntConversionFailure)
    }

    #[test]
    #[ignore = "Needs to run separately to not get influenced by other tests"]
    fn test_metrics() {
        let recorder = metrics_util::debugging::DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        recorder.install().unwrap();

        let snapshot = {
            let mut sandbox = ProtoWasmSandbox::default();
            sandbox
                .register(
                    "GetTimeSinceBootMicrosecond",
                    get_time_since_boot_microsecond,
                )
                .unwrap();

            let wasm_sandbox = sandbox.load_runtime().unwrap();
            let loaded_wasm_sandbox: LoadedWasmSandbox = {
                let mod_path = get_wasm_module_path("RunWasm.aot").unwrap();
                wasm_sandbox.load_module(mod_path).unwrap()
            };
            loaded_wasm_sandbox.unload_module().unwrap();
            snapshotter.snapshot()
        };
        let snapshot = snapshot.into_vec();
        if cfg!(feature = "function_call_metrics") {
            assert_eq!(snapshot.len(), 10);
        } else {
            assert_eq!(snapshot.len(), 8);
        }
    }
}
