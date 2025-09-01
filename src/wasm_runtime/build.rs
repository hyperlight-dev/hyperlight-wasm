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

use std::path::{Path, PathBuf};
use std::{env, fs};

use cargo_metadata::{MetadataCommand, Package};

fn main() {
    let toolchain_dir = env::var_os("HYPERLIGHT_GUEST_TOOLCHAIN_ROOT").unwrap();
    let toolchain_dir = PathBuf::from(toolchain_dir);
    let clang_path = toolchain_dir.join("clang");

    assert!(
        clang_path.exists(),
        "could not find clang at {clang_path:?}"
    );

    println!("cargo:rerun-if-changed=.");
    let mut cfg = cc::Build::new();

    // get the version of the wasmtime crate

    let metadata = MetadataCommand::new().exec().unwrap();
    let wasmtime_package: Option<&Package> =
        metadata.packages.iter().find(|p| *p.name == "wasmtime");
    let version_number = match wasmtime_package {
        Some(pkg) => pkg.version.clone(),
        None => panic!("wasmtime dependency not found"),
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
    cfg.compiler(clang_path);
    if cfg!(windows) {
        env::set_var("AR_x86_64_unknown_none", "llvm-ar");
    }
    cfg.compile("wasm_runtime");

    println!("cargo::rerun-if-env-changed=WIT_WORLD");
    println!("cargo::rustc-check-cfg=cfg(component)");
    if env::var_os("WIT_WORLD").is_some() {
        println!("cargo::rustc-cfg=component");
    }
}
