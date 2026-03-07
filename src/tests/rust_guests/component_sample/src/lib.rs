#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use bindings::component_sample::example::host::{host_function, print};
use bindings::exports::component_sample::example::adder::Guest;

struct Component {}

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
}

bindings::export!(Component with_types_in bindings);
