#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use bindings::exports::wasi_sample::example::adder::Guest;
use bindings::wasi_sample::example::host::{host_function, print};

struct Component {}

impl Guest for Component {
    fn add(left: u32, right: u32) -> u32 {
        left + right
    }

    fn call_host(input: String) -> String {
        let host_result = host_function(&format!("{} from component", &input));
        print(&host_result);
        host_result.to_string()
    }
}

bindings::export!(Component with_types_in bindings);
