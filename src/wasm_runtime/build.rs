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

use std::fs::File;
use std::io::copy;
use std::path::Path;
use std::{env, fs, path};

use cargo_metadata::{MetadataCommand, Package};
use reqwest::blocking::get;

fn main() {
    println!("cargo:rerun-if-changed=.");
    let mut cfg = cc::Build::new();
    if let Some(path) = env::var_os("PATH") {
        let paths: Vec<_> = env::split_paths(&path).collect();
        let toolchain_path =
            path::PathBuf::from(env::var_os("HYPERLIGHT_GUEST_TOOLCHAIN_ROOT").unwrap());
        let joined = env::join_paths(std::iter::once(toolchain_path).chain(paths)).unwrap();
        env::set_var("PATH", &joined);
    }

    // Get the wasmtime_platform.h file from the appropriate wasm release

    // get the version of the wasmtime crate

    let metadata = MetadataCommand::new().exec().unwrap();
    let wasmtime_package: Option<&Package> =
        metadata.packages.iter().find(|p| p.name == "wasmtime");
    let version_number = match wasmtime_package {
        Some(pkg) => pkg.version.clone(),
        None => panic!("wasmtime dependency not found"),
    };

    // download the wasmtime wasm_platform.h header file from GitHub releases and write it to the src/include directory

    // ensure the include directory exists
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let include_file = path::Path::new(&crate_dir)
        .join("src")
        .join("include")
        .join("wasmtime-platform.h");
    std::fs::create_dir_all(include_file.parent().unwrap()).unwrap();

    // get the wasm_platform.h header file from github release with matching version e.g. https://github.com/bytecodealliance/wasmtime/releases/download/v29.0.1/wasmtime-platform.h
    let path = format!(
        "https://github.com/bytecodealliance/wasmtime/releases/download/v{}/wasmtime-platform.h",
        version_number
    );
    let response = get(&path).expect("Failed to download header file");

    // write the include file to the src/include directory
    let out_file = include_file.to_str().unwrap();
    let mut dest = File::create(out_file).expect("Failed to create header file");
    copy(
        &mut response.bytes().expect("Failed to read response").as_ref(),
        &mut dest,
    )
    .expect("Failed to copy content");

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
    cfg.compiler("clang");
    if cfg!(windows) {
        env::set_var("AR_x86_64_unknown_none", "llvm-ar");
    }
    cfg.compile("wasm_runtime");

    println!("cargo::rerun-if-env-changed=HYPERLIGHT_WASM_WORLD");
    println!("cargo::rustc-check-cfg=cfg(component)");
    if env::var_os("HYPERLIGHT_WASM_WORLD").is_some() {
        println!("cargo::rustc-cfg=component");
    }
}
