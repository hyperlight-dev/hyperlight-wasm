#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use std::sync::atomic::{AtomicU32, Ordering};

use bindings::component_sample::example::host::{host_function, print};
use bindings::exports::component_sample::example::adder::Guest;

struct Component {}

/// State that a snapshot restore resets.
static COUNTER: AtomicU32 = AtomicU32::new(0);

impl Guest for Component {
    fn add(left: u32, right: u32) -> u32 {
        left + right
    }

    fn call_host(input: String) -> String {
        let host_result = host_function(&format!("{} from component", &input));
        host_result.to_string()
    }

    fn do_something(number: u32) {
        print(&format!("{number}"));
    }

    fn increment() -> u32 {
        COUNTER.fetch_add(1, Ordering::Relaxed) + 1
    }
}

bindings::export!(Component with_types_in bindings);
