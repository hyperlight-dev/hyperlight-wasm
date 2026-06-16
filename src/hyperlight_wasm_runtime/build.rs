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

use std::path::Path;
use std::{env, fs};

use cargo_metadata::{CargoOpt, MetadataCommand};

fn main() {
    if env::var_os("CARGO_CFG_HYPERLIGHT").is_none() {
        // we are not building for hyperlight, this is a no-op
        return;
    }

    println!("cargo:rerun-if-changed=.");
    let mut cfg = cc::Build::new();

    // get the version of the wasmtime crate
    // The LTS wasmtime version is the default; wasmtime_latest opts into the latest version.
    let use_lts = env::var("CARGO_FEATURE_WASMTIME_LATEST").is_err();
    let wasmtime_dep_name = if use_lts { "wasmtime_lts" } else { "wasmtime" };
    let mut metadata_cmd = MetadataCommand::new();
    if !use_lts {
        metadata_cmd
            .features(CargoOpt::NoDefaultFeatures)
            .features(CargoOpt::SomeFeatures(vec!["wasmtime_latest".to_string()]));
    }
    let metadata = metadata_cmd.exec().unwrap();
    let runtime_package = metadata
        .packages
        .iter()
        .find(|p| *p.name == "hyperlight-wasm-runtime")
        .expect("hyperlight-wasm-runtime package not found in cargo metadata");
    let resolve = metadata
        .resolve
        .as_ref()
        .expect("cargo metadata did not include dependency resolution");
    let runtime_node = resolve
        .nodes
        .iter()
        .find(|node| node.id == runtime_package.id)
        .expect("hyperlight-wasm-runtime dependency node not found in cargo metadata");
    let wasmtime_package_id = &runtime_node
        .deps
        .iter()
        .find(|dep| dep.name == wasmtime_dep_name)
        .unwrap_or_else(|| panic!("{wasmtime_dep_name} dependency not found in cargo metadata"))
        .pkg;
    let version_number = metadata[wasmtime_package_id].version.clone();

    // Write the version number to the metadata.rs file so that it is included in the binary

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("metadata.rs");

    // pad out the version number string with null bytes to 32 bytes
    let version_number_string = format!("{:\0<32}", version_number.to_string());

    let file_contents = format!(
        r#"
    // The section name beginning with .note is important, otherwise the linker will not include it in the binary.
    #[used]
    #[link_section = ".note_hyperlight_metadata"]
    static WASMTIME_VERSION_NUMBER: [u8; 32] = *b"{}";
    "#,
        version_number_string
    );
    fs::write(dest_path, file_contents).unwrap();

    cfg.include("src/include");
    cfg.file("src/platform.c");
    if cfg!(windows) {
        env::set_var("AR_x86_64_unknown_none", "llvm-ar");
    }
    cfg.compile("wasmtime-hyperlight-platform");

    println!("cargo::rerun-if-env-changed=WIT_WORLD");
    println!("cargo::rerun-if-env-changed=WIT_WORLD_NAME");
    println!("cargo::rustc-check-cfg=cfg(component)");
    if env::var_os("WIT_WORLD").is_some() {
        println!("cargo::rustc-cfg=component");
    }

    cfg_aliases::cfg_aliases! {
        gdb: { all(feature = "gdb", debug_assertions) },
        pulley: { feature = "pulley" },
    }
}
