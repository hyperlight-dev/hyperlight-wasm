use criterion::{Bencher, Criterion, criterion_group, criterion_main};
use hyperlight_host::HyperlightError;
use hyperlight_wasm::{LoadedWasmSandbox, Result, SandboxBuilder};

fn wasm_component_guest_call_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_component_guest_functions");

    // let bench_guest_function = |b: &mut Bencher<'_>, ext| {
    //     let mut loaded_wasm_sandbox = get_loaded_wasm_sandbox(ext);

    //     b.iter(|| {
    //         loaded_wasm_sandbox
    //             .call_guest_function::<String>("Echo", "Hello World!".to_string())
    //             .unwrap()
    //     });
    // };

    // group.bench_function("wasm_guest_call", |b: &mut Bencher<'_>| {
    //     bench_guest_function(b, "wasm");
    // });

    // group.bench_function("wasm_guest_call_aot", |b: &mut Bencher<'_>| {
    //     bench_guest_function(b, "aot");
    // });

    group.finish();
}

fn wasm_component_sandbox_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_component_sandboxes");
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

    let wasm_sandbox = sandbox.load_runtime().unwrap();

    wasm_sandbox
        .load_module(format!("../../x64/release/component_sample.aot",))
        .unwrap()
}

criterion_group! {
    name = benches_components;
    config = Criterion::default();//.warm_up_time(Duration::from_millis(50)); // If warm_up_time is default 3s warmup, the benchmark will fail due memory error
    targets = wasm_component_guest_call_benchmark, wasm_component_sandbox_benchmark
}
criterion_main!(benches_components);