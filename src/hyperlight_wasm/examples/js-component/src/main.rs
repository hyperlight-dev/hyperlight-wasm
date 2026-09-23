#![allow(renamed_and_removed_lints)]
#![allow(unknown_lints)]
#![allow(unused_unit)]

use std::sync::{Arc, Mutex};

use bindings::example::hello::Guest;

extern crate alloc;

mod bindings {
    hyperlight_component_macro::host_bindgen!({
        path: "wit/hello-world.wasm",
        world_name: "hello",
    });
}

#[derive(Clone)]
struct State {
    printed: Arc<Mutex<Vec<String>>>,
}

impl bindings::example::hello::Host for State {
    fn r#print(&mut self, message: alloc::string::String) {
        println!("guest print: {message}");
        self.printed.lock().unwrap().push(message);
    }
}

#[allow(refining_impl_trait)]
impl bindings::example::hello::HelloImports for State {
    type Host = State;

    fn r#host(&mut self) -> &mut Self {
        self
    }
}

fn main() {
    let printed = Arc::new(Mutex::new(Vec::new()));
    let state = State {
        printed: Arc::clone(&printed),
    };

    let mut sandbox = hyperlight_wasm::SandboxBuilder::new()
        .with_guest_input_buffer_size(70_000_000)
        .with_guest_heap_size(200_000_000)
        .with_guest_scratch_size(100 * 1024 * 1024)
        .build()
        .unwrap();

    let rt = bindings::register_host_functions(&mut sandbox, state);
    let sandbox = sandbox.load_runtime().unwrap();
    let sandbox = sandbox.load_module(env!("QJS_COMPONENT_AOT")).unwrap();

    let mut wrapped = bindings::HelloSandbox { sb: sandbox, rt };
    let instance = bindings::example::hello::HelloExports::guest(&mut wrapped);

    let result = instance.greet("World".to_string());
    assert_eq!("Hello, World!", result);
    println!("greet(\"World\") = {result}");

    let report = instance.describe_words(vec![
        "hyperlight".to_string(),
        "runs".to_string(),
        "javascript".to_string(),
    ]);
    assert_eq!(3, report.count);
    assert_eq!("hyperlight", report.longest);
    assert_eq!("HYPERLIGHT | RUNS | JAVASCRIPT", report.joined);

    println!(
        "describe-words(...) = count: {}, longest: {}, joined: {}",
        report.count, report.longest, report.joined
    );

    let invoice = instance.invoice_total(vec![20.0, 22.5, 7.5], 0.2).unwrap();
    assert_eq!(50.0, invoice.subtotal);
    assert_eq!(10.0, invoice.tax);
    assert_eq!(60.0, invoice.total);

    println!(
        "invoice-total(...) = subtotal: {}, tax: {}, total: {}",
        invoice.subtotal, invoice.tax, invoice.total
    );

    let err = instance.invoice_total(Vec::new(), 0.2).unwrap_err();
    assert_eq!("invoice must contain at least one amount", err);
    println!("invoice-total(empty, 0.2) threw and became err: {err}");

    let printed = printed.lock().unwrap();
    assert_eq!(
        *printed,
        vec!["JavaScript guest says hello to World".to_string()]
    );
}
