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

#![no_std]
#![no_main]

extern crate alloc;

// Re-export wasmtime based on expected version
#[cfg(all(feature = "wasmtime_latest", feature = "wasmtime_lts"))]
compile_error!(
    "Features 'wasmtime_latest' and 'wasmtime_lts' are mutually exclusive. Please enable only one."
);

#[cfg(not(any(feature = "wasmtime_latest", feature = "wasmtime_lts")))]
compile_error!("Either 'wasmtime_latest' or 'wasmtime_lts' feature must be enabled.");

#[cfg(not(feature = "wasmtime_lts"))]
extern crate wasmtime;
#[cfg(feature = "wasmtime_lts")]
extern crate wasmtime_lts as wasmtime;

mod platform;

#[cfg(not(component))]
mod hostfuncs;
#[cfg(not(component))]
mod marshal;
#[cfg(not(component))]
mod module;
#[cfg(not(component))]
mod wasip1;

#[cfg(component)]
mod component;

// The file referenced in this include! macro is created by the
// build.rs script.  The build.rs script gets the current version of
// wasmtime that this runtime binary uses, and writes it to the
// metadata.rs file so that it is embedded as metadata in the
// wasm_runtime binary. This allows hyperlight-wasm to then this
// metadata and log it when the hyperlight-wasm crate is loaded.

include!(concat!(env!("OUT_DIR"), "/metadata.rs"));
