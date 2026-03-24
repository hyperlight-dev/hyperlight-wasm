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

use cargo_metadata::{MetadataCommand, Package};

fn main() {
    if env::var_os("CARGO_CFG_HYPERLIGHT").is_none() {
        // we are not building for hyperlight, this is a no-op
        return;
    }

    println!("cargo:rerun-if-changed=.");
    let mut cfg = cc::Build::new();

    // get the version of the wasmtime crate
    // When wasmtime_lts feature is enabled, find the wasmtime version matching 36.x (LTS);
    // otherwise find the latest version.
    let metadata = MetadataCommand::new().exec().unwrap();
    let wasmtime_packages: Vec<&Package> = metadata
        .packages
        .iter()
        .filter(|p| *p.name == "wasmtime")
        .collect();

    let use_lts = env::var("CARGO_FEATURE_WASMTIME_LTS").is_ok();
    let version_number = if wasmtime_packages.len() == 1 {
        wasmtime_packages[0].version.clone()
    } else {
        // Multiple wasmtime versions present; pick based on feature
        wasmtime_packages
            .iter()
            .find(|p| {
                if use_lts {
                    p.version.major <= 36
                } else {
                    p.version.major > 36
                }
            })
            .unwrap_or_else(|| panic!("wasmtime dependency not found for lts={}", use_lts))
            .version
            .clone()
    };

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
