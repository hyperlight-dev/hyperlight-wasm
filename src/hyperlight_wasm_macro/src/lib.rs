/*
Copyright 2024 The Hyperlight Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

extern crate proc_macro;

use std::path::PathBuf;

use hyperlight_component_util::*;
mod wasmguest;

/// Create the hyperlight_guest_wasm_init() function (called by
/// hyperlight-wasm-runtime:component.rs) for the WIT world located at
/// $WIT_WORLD. Files ending in `.wit` are read as text WIT; other paths
/// are read as wasm-encoded WIT. This function registers Hyperlight
/// functions for component exports (which are implemented by calling
/// into wasmtime) and registers wasmtime host functions with the
/// wasmtime linker for component imports (which are implemented by
/// calling to the Hyperlight host).
///
/// If the WIT file contains multiple worlds, set the `WIT_WORLD_NAME`
/// environment variable to select a specific world by name. If not set,
/// the last world in the file will be used.
#[proc_macro]
pub fn wasm_guest_bindgen(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let path = PathBuf::from(std::env::var_os("WIT_WORLD").unwrap());
    let world_name = std::env::var("WIT_WORLD_NAME").ok();
    util::read_wit_type(wit_source(path), world_name, |kebab_name, ct| {
        let decls = emit::run_state(true, true, |s| {
            // Emit type/trait definitions for all instances in the world
            rtypes::emit_toplevel(s, &kebab_name, ct);
            // Emit the host/guest function registrations
            wasmguest::emit_toplevel(s, &kebab_name, ct);
        });
        // Use util::emit_decls() to choose between emitting the token
        // stream directly and emitting an include!() pointing at a
        // temporary file, depending on whether the user has requested
        // a debug temporary file be created.
        util::emit_decls(decls, &kebab_name).into()
    })
}

fn wit_source(path: PathBuf) -> util::WitSource {
    if path.extension().is_some_and(|extension| extension == "wit") {
        util::WitSource::Wit(path)
    } else {
        util::WitSource::Wasm(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wit_files_use_text_input() {
        let path = PathBuf::from("world.wit");
        assert!(matches!(
            wit_source(path.clone()),
            util::WitSource::Wit(source) if source == path
        ));
    }

    #[test]
    fn other_files_keep_wasm_input() {
        for filename in ["world.wasm", "world.bin", "world"] {
            let path = PathBuf::from(filename);
            assert!(matches!(
                wit_source(path.clone()),
                util::WitSource::Wasm(source) if source == path
            ));
        }
    }
}
