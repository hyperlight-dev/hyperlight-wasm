use examples_common::get_wasm_module_path;
use hyperlight_wasm::SandboxBuilder;

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

fn main() {
    let state = State::new();
    let mut sandbox = SandboxBuilder::new()
        //.with_debugging_enabled(8080)
        .build()
        .unwrap();
    let rt = bindings::register_host_functions(&mut sandbox, state);

    let sb = sandbox.load_runtime().unwrap();

    let mod_path = get_wasm_module_path("runcomponent.aot").unwrap();
    let sb = sb.load_module(mod_path).unwrap();

    let mut wrapped = bindings::RuncomponentSandbox { sb, rt };
    let instance = bindings::example::runcomponent::RuncomponentExports::guest(&mut wrapped);
    let echo = instance.echo("Hello World!".to_string());
    println!("{}", echo);

    let result = instance.round_to_nearest_int(1.331, 24.0);
    println!("rounded result {}", result);
    assert_eq!(result, 32);
}
