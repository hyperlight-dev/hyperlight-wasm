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

#![deny(dead_code, missing_docs, unused_mut)]
//! This crate provides a Hyperlight implementation for WebAssembly (Wasm) guest code.

/// provides details about the build
pub mod build_info;
mod sandbox;

use build_info::BuildInfo;
pub use sandbox::loaded_wasm_sandbox::LoadedWasmSandbox;
pub use sandbox::proto_wasm_sandbox::ProtoWasmSandbox;
pub use sandbox::sandbox_builder::SandboxBuilder;
pub use sandbox::wasm_sandbox::WasmSandbox;
/// The container to store the value of a single parameter to a guest
/// function.
pub type ParameterValue = hyperlight_host::func::ParameterValue;
/// The container to store the return value from a guest function call.
pub type ReturnValue = hyperlight_host::func::ReturnValue;
/// The type of the return value from a guest function call.
pub type ReturnType = hyperlight_host::func::ReturnType;
/// The Result of a fuunction call
pub type Result<T> = hyperlight_host::Result<T>;
/// Check if there is a hypervisor present
pub use hyperlight_host::is_hypervisor_present;
/// Create a generic HyperlightError
pub use hyperlight_host::new_error;
// A snapshot of the memory of a sandbox at a given point in time.
pub use hyperlight_host::sandbox::snapshot::Snapshot;

/// Get the build information for this version of hyperlight-wasm
pub fn get_build_info() -> BuildInfo {
    BuildInfo::get()
}
/// Get the wasmtime version used by this version of hyperlight-wasm
pub fn get_wasmtime_version() -> &'static str {
    BuildInfo::get_wasmtime_version()
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::Path;

    // Test that the build info is correct
    #[test]
    fn test_build_info() {
        let build_info = super::get_build_info();
        // calculate the blake3 hash of the wasm_runtime binary
        let wasm_runtime_hash = blake3::hash(&super::sandbox::WASM_RUNTIME);
        // check that the build info hash matches the wasm_runtime hash
        assert_eq!(
            build_info.wasm_runtime_blake3_hash,
            &wasm_runtime_hash.to_string()
        );
        assert_eq!(build_info.package_version, env!("CARGO_PKG_VERSION"));
    }
    // Test that the wasmtime version is correct
    #[test]
    fn test_wasmtime_version() {
        let wasmtime_version = super::get_wasmtime_version();
        // get the wasmtime version from the wasm_runtime binary's Cargo.toml
        let cargo_toml_path = Path::new(env!("OUT_DIR"))
            .join("vendor")
            .join("wasm_runtime")
            .join("Cargo.toml");
        let cargo_toml_content =
            std::fs::read_to_string(cargo_toml_path).expect("Failed to read Cargo.toml");
        let cargo_toml: toml::Value =
            toml::from_str(&cargo_toml_content).expect("Failed to parse Cargo.toml");
        let wasmtime_version_from_toml = cargo_toml
            .get("dependencies")
            .and_then(|deps| deps.get("wasmtime"))
            .and_then(|wasmtime| wasmtime.get("version"))
            .and_then(|version| version.as_str())
            .expect("Failed to find wasmtime version in Cargo.toml");
        assert_eq!(wasmtime_version, wasmtime_version_from_toml);
    }
}
