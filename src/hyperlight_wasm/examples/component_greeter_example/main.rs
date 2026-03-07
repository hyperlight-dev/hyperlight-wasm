#![allow(renamed_and_removed_lints)]
#![allow(unknown_lints)]
#![allow(unused_unit)]

use bindings::component_sample::example::Greeter;
use examples_common::get_wasm_module_path;

extern crate alloc;
mod bindings {
    // Demonstrate world selection from a multi-world WIT package:
    // The same component-world.wasm contains both "example" and "greeter-world",
    // and we select "greeter-world" via world_name.
    // Both worlds import the same "host" interface, showing interface reuse.
    hyperlight_component_macro::host_bindgen!({
        path: "../component_sample/wit/component-world.wasm",
        world_name: "greeter-world",
    });
}

pub struct State {
    prefix: String,
}

impl State {
    pub fn new(prefix: &str) -> Self {
        State {
            prefix: prefix.to_string(),
        }
    }
}

// Same Host trait as component_example — shared interface across worlds
impl bindings::component_sample::example::Host for State {
    fn r#print(&mut self, message: alloc::string::String) {
        println!("[log] {message}");
    }

    fn r#host_function(&mut self, input: alloc::string::String) -> alloc::string::String {
        if input == "prefix" {
            self.prefix.clone()
        } else {
            format!("{input} and the host!")
        }
    }
}

#[allow(refining_impl_trait)]
impl bindings::component_sample::example::GreeterWorldImports for State {
    type Host = State;

    fn r#host(&mut self) -> &mut Self {
        self
    }
}

fn main() {
    let state = State::new("Hello");
    let mut sb: hyperlight_wasm::ProtoWasmSandbox = hyperlight_wasm::SandboxBuilder::new()
        .with_guest_input_buffer_size(70000000)
        .with_guest_heap_size(200000000)
        .with_guest_scratch_size(100 * 1024 * 1024)
        .build()
        .unwrap();
    let rt = bindings::register_host_functions(&mut sb, state);

    let sb = sb.load_runtime().unwrap();

    let mod_path = get_wasm_module_path("greeter_sample.aot").unwrap();
    let sb = sb.load_module(mod_path).unwrap();

    let mut wrapped = bindings::GreeterWorldSandbox { sb, rt };

    let instance = bindings::component_sample::example::GreeterWorldExports::greeter(&mut wrapped);

    let result = instance.greet("World".to_string());
    assert_eq!("Hello World!", result);
    println!("Greet result: {result}");

    let result = instance.greet("Hyperlight".to_string());
    assert_eq!("Hello Hyperlight!", result);
    println!("Greet result: {result}");
}
