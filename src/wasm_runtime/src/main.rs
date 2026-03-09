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

#![cfg_attr(hyperlight, no_std)]
#![cfg_attr(hyperlight, no_main)]

// Since we are not explicitly using anything from the library, we need to
// explicitly import it to ensure it is linked in.
extern crate wasm_runtime;

#[cfg(not(hyperlight))]
fn main() {
    panic!("This is the hyperlight-wasm-runtime crate. It is not meant to be run outside of a hyperlight sandbox.");
}
