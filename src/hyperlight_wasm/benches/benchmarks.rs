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

use criterion::{Bencher, Criterion, criterion_group, criterion_main};
use hyperlight_host::HyperlightError;
use hyperlight_wasm::{LoadedWasmSandbox, Result, SandboxBuilder};

fn get_time_since_boot_microsecond() -> Result<i64> {
    let res = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)?
        .as_micros();
    i64::try_from(res).map_err(HyperlightError::IntConversionFailure)
}

fn wasm_guest_call_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_guest_functions");

    let bench_guest_function = |b: &mut Bencher<'_>, ext| {
        let mut loaded_wasm_sandbox = get_loaded_wasm_sandbox(ext);

        b.iter(|| {
            loaded_wasm_sandbox
                .call_guest_function::<String>("Echo", "Hello World!".to_string())
                .unwrap()
        });
    };

    group.bench_function("wasm_guest_call", |b: &mut Bencher<'_>| {
        bench_guest_function(b, "wasm");
    });

    group.bench_function("wasm_guest_call_aot", |b: &mut Bencher<'_>| {
        bench_guest_function(b, "aot");
    });

    group.finish();
}

fn wasm_sandbox_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_sandboxes");
    let create_wasm_sandbox = || {
        get_loaded_wasm_sandbox("wasm");
    };

    group.bench_function("create_sandbox", |b| {
        b.iter_with_large_drop(create_wasm_sandbox);
    });

    group.bench_function("create_sandbox_and_drop", |b| {
        b.iter(create_wasm_sandbox);
    });

    group.finish();
}

fn get_loaded_wasm_sandbox(ext: &str) -> LoadedWasmSandbox {
    let mut sandbox = SandboxBuilder::new().build().unwrap();

    sandbox
        .register(
            "GetTimeSinceBootMicrosecond",
            get_time_since_boot_microsecond,
        )
        .unwrap();

    let wasm_sandbox = sandbox.load_runtime().unwrap();

    wasm_sandbox
        .load_module(format!("../../x64/release/RunWasm.{ext}",))
        .unwrap()
}

criterion_group! {
    name = benches;
    config = Criterion::default();//.warm_up_time(Duration::from_millis(50)); // If warm_up_time is default 3s warmup, the benchmark will fail due memory error
    targets = wasm_guest_call_benchmark, wasm_sandbox_benchmark
}
criterion_main!(benches);
