use std::sync::{Arc, Mutex};

use criterion::{Bencher, Criterion, criterion_group, criterion_main};
use hyperlight_wasm::{LoadedWasmSandbox, SandboxBuilder};

use crate::bindings::example::runcomponent::Guest;

extern crate alloc;
mod bindings {
    hyperlight_component_macro::host_bindgen!(
        "../../src/wasmsamples/components/runcomponent-world.wasm"
    );
}

pub struct State {}
impl State {
    pub fn new() -> Self {
        State {}
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl bindings::example::runcomponent::Host for State {
    fn r#get_time_since_boot_microsecond(&mut self) -> i64 {
        let res = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_micros();
        i64::try_from(res).unwrap()
    }
}

impl bindings::example::runcomponent::RuncomponentImports for State {
    type Host = State;

    fn r#host(&mut self) -> impl ::core::borrow::BorrowMut<Self::Host> {
        self
    }
}

fn wasm_component_guest_call_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_component_guest_functions");

    let bench_guest_function = |b: &mut Bencher<'_>| {
        let (sb, rt) = get_loaded_wasm_sandbox();
        let mut wrapped = bindings::RuncomponentSandbox { sb, rt };
        let instance = bindings::example::runcomponent::RuncomponentExports::guest(&mut wrapped);

        b.iter(|| {
            instance.echo("Hello World!".to_string());
        });
    };

    group.bench_function("wasm_guest_call_aot", |b: &mut Bencher<'_>| {
        bench_guest_function(b);
    });

    group.finish();
}

fn wasm_component_sandbox_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_component_sandboxes");
    let create_wasm_sandbox = || {
        get_loaded_wasm_sandbox();
    };

    group.bench_function("create_sandbox", |b| {
        b.iter_with_large_drop(create_wasm_sandbox);
    });

    group.bench_function("create_sandbox_and_drop", |b| {
        b.iter(create_wasm_sandbox);
    });

    group.finish();
}

fn get_loaded_wasm_sandbox() -> (
    LoadedWasmSandbox,
    Arc<Mutex<bindings::RuncomponentResources<State>>>,
) {
    let state = State::new();
    let mut sandbox = SandboxBuilder::new().build().unwrap();
    let rt = bindings::register_host_functions(&mut sandbox, state);

    let sb = sandbox.load_runtime().unwrap();

    let sb = sb
        .load_module("../../x64/release/runcomponent.aot")
        .unwrap();
    (sb, rt)
}

criterion_group! {
    name = benches_components;
    config = Criterion::default();//.warm_up_time(Duration::from_millis(50)); // If warm_up_time is default 3s warmup, the benchmark will fail due memory error
    targets = wasm_component_guest_call_benchmark, wasm_component_sandbox_benchmark
}
criterion_main!(benches_components);
