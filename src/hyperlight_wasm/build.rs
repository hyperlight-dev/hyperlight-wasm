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

// build.rs

// The purpose of this build script is to embed the wasm_runtime binary as a resource in the hyperlight-wasm binary.
// This is done by reading the wasm_runtime binary into a static byte array named WASM_RUNTIME.
// this build script writes the code to do that to a file named wasm_runtime_resource.rs in the OUT_DIR.
// this file is included in lib.rs.
// The wasm_runtime binary is expected to be in the x64/{config} directory.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::Result;
use built::write_built_file;

fn build_wasm_runtime() -> PathBuf {
    let proj_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let profile = env::var_os("PROFILE").unwrap();

    let in_repo_dir = PathBuf::from(&proj_dir)
        .parent()
        .unwrap_or_else(|| panic!("could not find parent of cargo manifest directory"))
        .join("wasm_runtime");
    if !in_repo_dir.exists() {
        panic!("hyperlight_wasm does not yet support being compiled from a release package");
    }
    print!("cargo::rerun-if-changed=");
    let _ = std::io::stdout()
        .write_all(AsRef::<std::ffi::OsStr>::as_ref(&in_repo_dir).as_encoded_bytes());
    println!();
    println!("cargo::rerun-if-env-changed=HYPERLIGHT_WASM_WORLD");
    // the PROFILE env var unfortunately only gives us 1 bit of "dev or release"
    let cargo_profile = if profile == "debug" { "dev" } else { "release" };
    let mut cmd = std::process::Command::new("cargo");

    // Clear the variables that control Cargo's behaviour (as listed
    // at https://doc.rust-lang.org/cargo/reference/environment-variables.html):
    // otherwise the nested build will build the wrong thing
    let mut env_vars = env::vars().collect::<Vec<_>>();
    env_vars.retain(|(key, _)| !key.starts_with("CARGO_"));

    let cmd = cmd
        .args(["build", &format!("--profile={}", cargo_profile), "-v"])
        .current_dir(&in_repo_dir)
        .env_clear()
        .envs(env_vars);

    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("could not run cargo build: {}", e));
    if !status.success() {
        panic!("could not compile wasm_runtime");
    }
    let resource = in_repo_dir
        .join("target")
        .join("x86_64-unknown-none")
        .join(profile)
        .join("wasm_runtime");

    resource.canonicalize().unwrap_or_else(|_| {
        panic!(
            "could not find wasm_runtime after building it (expected {:?})",
            resource
        )
    })
}

fn main() -> Result<()> {
    let wasm_runtime_resource = build_wasm_runtime();

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("wasm_runtime_resource.rs");
    let contents = format!(
        "pub (super) static WASM_RUNTIME: [u8; include_bytes!({name:?}).len()] = *include_bytes!({name:?});",
        name = wasm_runtime_resource.as_os_str()
    );

    fs::write(dest_path, contents).unwrap();

    // get the wasmtime version number from the wasm_runtime metadata

    let wasm_runtime_bytes = fs::read(&wasm_runtime_resource).unwrap();
    let elf = goblin::elf::Elf::parse(&wasm_runtime_bytes).unwrap();

    // the wasm_runtime binary has a section named .note_hyperlight_metadata that contains the wasmtime version number
    // this section is added to the wasm_runtime binary by the build.rs script in the wasm_runtime crate
    let section_name = ".note_hyperlight_metadata";
    let wasmtime_version_number = if let Some(header) = elf.section_headers.iter().find(|hdr| {
        if let Some(name) = elf.shdr_strtab.get_at(hdr.sh_name) {
            name == section_name
        } else {
            false
        }
    }) {
        let start = header.sh_offset as usize;
        let size = header.sh_size as usize;
        let end = start + size;
        let metadata_bytes = &wasm_runtime_bytes[start..end];
        // convert the metadata bytes to a string
        if let Some(null_pos) = metadata_bytes.iter().position(|&b| b == 0) {
            std::str::from_utf8(&metadata_bytes[..null_pos]).unwrap()
        } else {
            std::str::from_utf8(metadata_bytes).unwrap()
        }
    } else {
        panic!(".note_hyperlight_metadata section not found in wasm_runtime binary");
    };

    // write the build information to the built.rs file
    write_built_file()?;

    // open the built.rs file and append the details of the wasm_runtime file
    let built_path = Path::new(&out_dir).join("built.rs");
    let mut file = OpenOptions::new()
        .create(false)
        .append(true)
        .open(built_path)
        .unwrap();

    let metadata = fs::metadata(&wasm_runtime_resource).unwrap();
    let created = metadata.modified().unwrap();
    let created_datetime: chrono::DateTime<chrono::Local> = created.into();
    let wasm_runtime_created = format!(
        "static WASM_RUNTIME_CREATED: &str = \"{created_datetime}\";",
        created_datetime = created_datetime
    );

    let wasm_runtime_size = format!(
        "static WASM_RUNTIME_SIZE: &str = \"{size}\";",
        size = metadata.len()
    );

    let wasm_runtime_wasmtime_version = format!(
        "static WASM_RUNTIME_WASMTIME_VERSION: &str = \"{wasmtime_version_number}\";",
        wasmtime_version_number = wasmtime_version_number
    );

    writeln!(file, "{}", wasm_runtime_created).unwrap();
    writeln!(file, "{}", wasm_runtime_size).unwrap();
    writeln!(file, "{}", wasm_runtime_wasmtime_version).unwrap();

    // Calculate the blake3 hash of the wasm_runtime file and write it to the wasm_runtime_resource.rs file so we can include it in the binary
    let wasm_runtime = fs::read(wasm_runtime_resource).unwrap();
    let hash = blake3::hash(&wasm_runtime);
    let hash_str = format!("static WASM_RUNTIME_BLAKE3_HASH: &str = \"{}\";", hash);

    writeln!(file, "{}", hash_str).unwrap();

    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}
