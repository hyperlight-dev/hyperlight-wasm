#![allow(renamed_and_removed_lints)]
#![allow(unknown_lints)]
#![allow(unused_unit)]

use bindings::component_sample::example::Adder;
use examples_common::get_wasm_module_path;

extern crate alloc;
mod bindings {
    hyperlight_component_macro::host_bindgen!("../component_sample/wit/component-world.wasm");
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

impl bindings::component_sample::example::Host for State {
    fn r#print(&mut self, message: alloc::string::String) {
        assert_eq!("42", message);
        println!("Logged from component: {message}");
    }

    fn r#host_function(&mut self, input: alloc::string::String) -> alloc::string::String {
        format!("{input} and the host!")
    }
}

#[allow(refining_impl_trait)]
impl bindings::component_sample::example::ExampleImports for State {
    type Host = State;

    fn r#host(&mut self) -> &mut Self {
        self
    }
}

fn main() {
    let state = State::new();
    let mut sb: hyperlight_wasm::ProtoWasmSandbox = hyperlight_wasm::SandboxBuilder::new()
        .with_guest_input_buffer_size(70000000)
        .with_guest_heap_size(200000000)
        .with_guest_stack_size(100000000)
        .build()
        .unwrap();
    let rt = bindings::register_host_functions(&mut sb, state);

    let sb = sb.load_runtime().unwrap();

    let mod_path = get_wasm_module_path("component_sample.aot").unwrap();
    let sb = sb.load_module(mod_path).unwrap();

    let mut wrapped = bindings::ExampleSandbox { sb, rt };

    let instance = bindings::component_sample::example::ExampleExports::adder(&mut wrapped);
    let result = instance.add(1, 2);
    assert_eq!(3, result);
    println!("Add result is {result}");
    let result = instance.add(4, 3);
    assert_eq!(7, result);
    println!("Add result is {result}");
    instance.do_something(42);

    let result = instance.call_host("Hello".to_string());
    assert_eq!("Hello from component and the host!", result);
    print!("Host Component interaction: {result}")
}
