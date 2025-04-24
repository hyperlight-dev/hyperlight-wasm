# Observability

hyperlight-wasm provides the following observability features:

* [Metrics](#metrics) metrics are provided using `metrics` crate.

## Metrics


The following metrics are provided and are enabled by default:

* `active_proto_wasm_sandboxes` - A gauge indicating the number of currently active proto wasm sandboxes
* `active_wasm_sandboxes` - A gauge indicating the number of currently active wasm sandboxes
* `active_loaded_wasm_sandboxes` - A gauge indicating the number of currently loaded wasm sandboxes
* `proto_wasm_sandboxes_total` - A counter indicating the total number of proto wasm sandboxes created during the lifetime of the process
* `wasm_sandboxes_total` - A counter indicating the total number of wasm sandboxes created during the lifetime of the process
* `loaded_wasm_sandboxes_total` - A counter indicating the total number of loaded wasm sandboxes created during the lifetime of the process
* `sandbox_loads_total` - A counter indicating how many times a wasm sandbox has been loaded into a loaded wasm sandbox during the lifetime of the process
* `sandbox_unloads_total` - A counter indicating how many times a loaded wasm sandbox has been unloaded into a wasm sandbox during the lifetime of the process


In addition, regular Hyperlight provides the following metrics: 

* `guest_errors_total` - A counter indicating how many times a guest error has occurred
* `guest_cancellations_total` - The number of times guest execution has timed out

If cargo feature `function_call_metrics` is enabled:

* `guest_call_duration_seconds` - Histogram for the execution time of guest function calls
* `host_call_duration_seconds` - Histogram for the execution time of host function calls

There is an example of how to gather metrics in the [examples/metrics](../src/hyperlight_wasm/examples/metrics) directory.
