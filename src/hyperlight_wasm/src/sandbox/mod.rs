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

/// A Wasm Sandbox loaded with a module.
pub(crate) mod loaded_wasm_sandbox;
/// Metric definitions for Sandbox module.
pub(crate) mod metrics;
/// A builder for a WasmSandbox.
pub(crate) mod sandbox_builder;
/// A Wasm Sandbox that can load a module.
pub(crate) mod wasm_sandbox;

pub(crate) mod proto_wasm_sandbox;

// This include! macro is replaced by the build.rs script.
// The build.rs script reads the wasm_runtime binary into a static byte array named WASM_RUNTIME
// contained in the wasm_runtime_resource.rs file.

include!(concat!(env!("OUT_DIR"), "/wasm_runtime_resource.rs"));
