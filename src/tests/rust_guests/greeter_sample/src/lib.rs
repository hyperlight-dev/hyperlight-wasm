#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use bindings::component_sample::example::host::{host_function, print};
use bindings::exports::component_sample::example::greeter::Guest;

struct Component {}

impl Guest for Component {
    fn greet(name: String) -> String {
        let prefix = host_function("prefix");
        print(&format!("Greeting {name}"));
        format!("{prefix} {name}!")
    }
}

bindings::export!(Component with_types_in bindings);
