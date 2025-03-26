# Observability

hyperlight-wasm provides the following observability features:

* [Metrics](#metrics) metrics are provided using Prometheus.

## Metrics

Hyperlight-wasm provides metrics using Prometheus. The metrics are registered using either the [default_registry](https://docs.rs/prometheus/latest/prometheus/fn.default_registry.html) or a registry instance provided by the host application.

To provide a registry to hyperlight_wasm, use the `set_metrics_registry` function and pass a reference to a registry with `static` lifetime:

```rust
use hyperlight_wasm::set_metrics_registry;
use prometheus::Registry;
use lazy_static::lazy_static;

lazy_static! {
    static ref REGISTRY: Registry = Registry::new();
}

set_metrics_registry(&REGISTRY);
```

The following metrics are provided and are enabled by default:

* `hyperlight_guest_error_count` - a vector of counters that tracks the number of guest errors by code and message.
* `hyperlight_number_of_cancelled_guest_execution` - a counter that tracks the number of guest executions that have been cancelled because the execution time exceeded the time allowed.
* `current_number_of_wasm_sandboxes` - a gauge that tracks the current number of wasm sandboxes in this process.
* `current_number_of_loaded_wasm_sandboxes` - a gauge that tracks the current number of loaded wasm sandboxes in this process.
* `number_of_unloads_of_loaded_wasm_sandboxes` - a counter that tracks the number of times that unload_module has been called on a LoadedWasmSandbox.
* `number_of_loads_of_wasm_sandboxes` - a counter that tracks the number of times that load_module has been called on a WasmSandbox.
* `current_number_of_proto_wasm_sandboxes` - a gauge that tracks the current number of proto wasm sandboxes in this process.
* `total_number_of_wasm_sandboxes` - a counter that tracks the total number of wasm sandboxes that have been created by this process.
* `total_number_of_loaded_wasm_sandboxes` - a counter that tracks the total number of loaded wasm sandboxes that have been created by this process.
* `total_number_of_proto_wasm_sandboxes` - a counter that tracks the total number of proto wasm sandboxes that have been created by this process.

The following metrics are provided and are enabled by default using the feature `function_call_metrics` but can be disabled:

* `hyperlight_guest_function_call_duration_microseconds` - a vector of histograms that tracks the execution time of guest functions in microseconds by function name. The histogram also tracks the number of calls to each function.
* `hyperlight_host_function_calls_duration_microseconds` - a vector of histograms that tracks the execution time of host functions in microseconds by function name. The histogram also tracks the number of calls to each function.

There is an example of how to gather metrics in the [examples/metrics](../src/hyperlight_wasm/examples/metrics) directory.
