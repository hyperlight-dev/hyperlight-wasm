use std::ffi::CString;
use std::io;
use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex};

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use hyperlight_wasm::{LoadedWasmSandbox, SandboxBuilder, WasmSandbox};

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

#[cfg(not(windows))]
fn wasm_component_load_call_unload_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_component_load_call_unload");

    let aot_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../x64/release/runcomponent.aot"
    );
    let (mmap_base, mmap_len) = unsafe {
        let fd = libc::open(CString::new(aot_path).unwrap().as_ptr(), libc::O_RDONLY);
        assert!(
            fd >= 0,
            "couldn't open {}: {:?}",
            aot_path,
            io::Error::last_os_error()
        );

        let mut st = MaybeUninit::<libc::stat>::uninit();
        libc::fstat(fd, st.as_mut_ptr());
        let st = st.assume_init();

        let page_size = libc::sysconf(libc::_SC_PAGESIZE) as usize;
        let len = (st.st_size as usize).next_multiple_of(page_size);
        let base = libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            libc::MAP_PRIVATE,
            fd,
            0,
        );
        libc::close(fd);
        assert_ne!(
            base,
            libc::MAP_FAILED,
            "mmap failed: {:?}",
            io::Error::last_os_error()
        );
        (base, len)
    };

    group.bench_function("load_call_unload", |b: &mut Bencher<'_>| {
        let mut wasm_sandbox = Some({
            let state = State::new();
            let mut sandbox = SandboxBuilder::new().build().unwrap();
            let rt = bindings::register_host_functions(&mut sandbox, state);
            let sb = sandbox.load_runtime().unwrap();
            (sb, rt)
        });

        b.iter(|| {
            let (ws, rt) = wasm_sandbox.take().unwrap();
            let loaded = unsafe { ws.load_module_by_mapping(mmap_base, mmap_len).unwrap() };
            let mut wrapped = bindings::RuncomponentSandbox { sb: loaded, rt };
            let instance =
                bindings::example::runcomponent::RuncomponentExports::guest(&mut wrapped);
            instance.echo("Hello World!".to_string());
            let unloaded = wrapped.sb.unload_module().unwrap();
            wasm_sandbox = Some((unloaded, wrapped.rt));
        });
    });

    group.finish();

    unsafe {
        libc::munmap(mmap_base, mmap_len);
    }
}

fn get_loaded_wasm_sandbox() -> (
    LoadedWasmSandbox,
    Arc<Mutex<bindings::RuncomponentResources<State>>>,
) {
    let state = State::new();
    let mut sandbox = SandboxBuilder::new().build().unwrap();
    let rt = bindings::register_host_functions(&mut sandbox, state);

    let sb = sandbox.load_runtime().unwrap();

    let aot_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../x64/release/runcomponent.aot"
    );
    let sb = sb.load_module(aot_path).unwrap();
    (sb, rt)
}

#[cfg(not(windows))]
criterion_group! {
    name = benches_components;
    config = Criterion::default();//.warm_up_time(Duration::from_millis(50)); // If warm_up_time is default 3s warmup, the benchmark will fail due memory error
    targets = wasm_component_guest_call_benchmark, wasm_component_sandbox_benchmark, wasm_component_load_call_unload_benchmark
}

#[cfg(windows)]
criterion_group! {
    name = benches_components;
    config = Criterion::default();
    targets = wasm_component_guest_call_benchmark, wasm_component_sandbox_benchmark
}
criterion_main!(benches_components);
