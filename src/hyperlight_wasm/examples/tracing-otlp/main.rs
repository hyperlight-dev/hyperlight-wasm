/*
Copyright 2025  The Hyperlight Authors.

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
//! This example demonstrates how to use OpenTelemetry (OTel) tracing with the Hyperlight Wasm
//! sandbox.
//!
//!! It initializes an OTel tracing subscriber that exports traces to an OTLP endpoint, spawns multiple
//! threads that create Wasm sandboxes, load Wasm modules, and call functions within those modules while
//! generating tracing spans for each operation.
//!
//! To run this example, ensure you have an OTLP-compatible tracing backend running at the
//! specified endpoint.
//!
//! Prerequisites:
//! - An OTLP-compatible tracing backend (e.g., OpenTelemetry Collector, Jaeger,
//!   Zipkin) running and accessible at `http://localhost:4318/v1/traces`.
//! - The `rust_wasm_samples.aot` Wasm module available in the expected path (x64/<profile>/).
//! - The `trace_guest` feature enabled in the `hyperlight-wasm` crate.
//!
//! ```sh
//! cargo run --example tracing-otlp --features="trace_guest"
//! ```
//!
//!

#![allow(clippy::disallowed_macros)]

use std::error::Error;
use std::io::stdin;
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle, spawn};

use examples_common::get_wasm_module_path;
use hyperlight_wasm::{LoadedWasmSandbox, Result as HyperlightResult, SandboxBuilder};
use opentelemetry::trace::TracerProvider;
use opentelemetry::{KeyValue, global};
use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_semantic_conventions::attribute::SERVICE_VERSION;
use tracing::{Level, span};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use uuid::Uuid;

const ENDPOINT_ADDR: &str = "http://localhost:4318/v1/traces";

fn init_tracing_subscriber(
    addr: &str,
) -> std::result::Result<SdkTracerProvider, Box<dyn Error + Send + Sync + 'static>> {
    let exporter = SpanExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(addr)
        .build()?;

    let version = KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION"));
    let resource = Resource::builder()
        .with_service_name("hyperlight_wasm_otel_example")
        .with_attribute(version)
        .build();

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build();

    global::set_tracer_provider(provider.clone());
    let tracer = provider.tracer("trace-demo");

    let otel_layer = OpenTelemetryLayer::new(tracer);

    // Try using the environment otherwise set default filters
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::from_default_env()
            .add_directive("hyperlight_host=info".parse().unwrap())
            .add_directive("tracing=info".parse().unwrap())
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(otel_layer)
        .try_init()?;

    Ok(provider)
}

fn run_example(wait_input: bool) -> HyperlightResult<()> {
    type TestFn = fn(&mut LoadedWasmSandbox, &str, &str) -> HyperlightResult<i32>;
    let tests: &[(&str, &str, TestFn)] = &[
        (
            "rust_wasm_samples.aot",
            "hello_world",
            |sb, fn_name, module| {
                println!("Calling {} Function in Wasm Module {}", fn_name, module,);
                sb.call_guest_function(fn_name, ())
            },
        ),
        ("rust_wasm_samples.aot", "add", |sb, fn_name, module| {
            println!("Calling {} Function in Wasm Module {}", fn_name, module,);
            sb.call_guest_function(fn_name, (5i32, 3i32))
        }),
        (
            "rust_wasm_samples.aot",
            "call_host_function",
            |sb, fn_name, module| {
                println!("Calling {} Function in Wasm Module {}", fn_name, module,);
                sb.call_guest_function("call_host_function", 5i32)
            },
        ),
    ];

    let mut join_handles: Vec<JoinHandle<HyperlightResult<()>>> = vec![];

    // Construct a new span named "hyperlight otel tracing example" with INFO  level.
    let span = span!(Level::INFO, "hyperlight otel tracing example");
    let _entered = span.enter();

    let should_exit = Arc::new(Mutex::new(false));
    let host_func = |a: i32| {
        println!("host_func called with {}", a);
        a + 1
    };

    for i in 0..10 {
        let exit = Arc::clone(&should_exit);
        let handle = spawn(move || -> HyperlightResult<()> {
            while !*exit.try_lock().unwrap() {
                // Construct a new span named "hyperlight tracing example thread" with INFO  level.
                let id = Uuid::new_v4();
                let span = span!(
                    Level::INFO,
                    "hyperlight tracing example thread",
                    context = format!("Thread number {} GUID {}", i, id),
                    uuid = %id,
                );
                let _entered = span.enter();

                let mut sandbox = SandboxBuilder::new().build()?;

                sandbox.register("TestHostFunc", host_func)?;
                let mut wasm_sandbox = Some(sandbox.load_runtime()?);
                for (module, fn_name, func) in tests.iter() {
                    let mod_path = get_wasm_module_path(module)?;

                    // Load a Wasm module into the sandbox
                    let mut loaded_wasm_sandbox =
                        wasm_sandbox.take().unwrap().load_module(mod_path)?;

                    let _res = func(&mut loaded_wasm_sandbox, fn_name, module)?;
                    wasm_sandbox = Some(loaded_wasm_sandbox.unload_module()?);
                }

                // Set exit to true to exit the loop after one iteration for this example.
                *exit.try_lock().unwrap() = true;
            }
            Ok(())
        });
        join_handles.push(handle);
    }

    if wait_input {
        println!("Press enter to exit...");
        let mut input = String::new();
        stdin().read_line(&mut input)?;
    }

    *should_exit.try_lock().unwrap() = true;
    for join_handle in join_handles {
        let result = join_handle.join();
        assert!(result.is_ok());
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let provider = init_tracing_subscriber(ENDPOINT_ADDR)?;

    run_example(true)?;

    provider.shutdown()?;

    Ok(())
}
