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

// The purpose of this build script is to embed the hyperlight-wasm-runtime binary as a resource in the hyperlight-wasm binary.
// This is done by reading the hyperlight-wasm-runtime binary into a static byte array named WASM_RUNTIME.
// this build script writes the code to do that to a file named built.rs in the OUT_DIR.
// this file is included in lib.rs.
// The hyperlight-wasm-runtime binary is expected to be in the x64/{config} directory.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::Result;
use built::write_built_file;

fn get_wasm_runtime_manifest_path() -> PathBuf {
    // Use cargo metadata to obtain information about our dependencies
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = std::process::Command::new(&cargo)
        .args(["metadata", "--format-version=1"])
        .output()
        .expect("Cargo is not installed or not found in PATH");

    assert!(
        output.status.success(),
        "Failed to get cargo metadata: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Cargo metadata output is in JSON format, so we use serde_json to parse it.
    // The output will look like this:
    // {
    //     "packages": [
    //         ...,
    //         {
    //             "name": "hyperlight-wasm-runtime",
    //             "manifest_path": "/path/to/hyperlight-wasm-runtime/Cargo.toml",
    //             ...
    //         },
    //         ...
    //     ],
    //     ...
    // }
    // We only care about the name and manifest_path fields of the packages, so we
    // define a minimal struct to deserialize the output.
    #[derive(serde::Deserialize)]
    struct CargoMetadata {
        packages: Vec<CargoPackage>,
    }

    #[derive(serde::Deserialize)]
    struct CargoPackage {
        name: String,
        manifest_path: PathBuf,
    }

    let metadata: CargoMetadata =
        serde_json::from_slice(&output.stdout).expect("Failed to parse cargo metadata");

    // find the package entry for hyperlight-wasm-runtime and get its manifest_path
    let hyperlight_wasm_runtime = metadata
        .packages
        .into_iter()
        .find(|pkg| pkg.name == "hyperlight-wasm-runtime")
        .expect("hyperlight-wasm-runtime crate not found in cargo metadata");

    hyperlight_wasm_runtime.manifest_path
}

fn find_target_dir() -> PathBuf {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    let target = env::var("TARGET").unwrap();

    // out_dir is expected to be something like /path/to/target/(ARCH?)/debug/build/hyperlight_wasm-xxxx/out
    // move up until either ARCH or "target"
    let target_dir = out_dir
        .ancestors()
        .nth(4)
        .expect("OUT_DIR does not have enough ancestors to find target directory");

    // If the target directory is named after the target triple, move up one more level to get to the actual target directory
    // Also, check that the parent directory contains a CACHEDIR.TAG file to make sure we're in the right place
    if target_dir.file_name() == Some(target.as_str().as_ref())
        && let Some(parent) = target_dir.parent()
        && parent.join("CACHEDIR.TAG").exists()
    {
        return parent.to_path_buf();
    }

    target_dir.to_path_buf()
}

fn build_wasm_runtime() -> PathBuf {
    let profile = env::var_os("PROFILE").unwrap();

    // Get the current target directory.
    let target_dir = find_target_dir();
    // Do not use the target directory directly, as it is locked by cargo with the current build
    // and would result in a deadlock
    let target_dir = target_dir.join("hyperlight-wasm-runtime");

    let manifest_path = get_wasm_runtime_manifest_path();
    let runtime_dir = manifest_path.parent().unwrap();

    if !runtime_dir.exists() {
        panic!("missing hyperlight-wasm-runtime in-tree dependency");
    }

    println!("cargo::rerun-if-changed={}", runtime_dir.display());
    println!("cargo::rerun-if-env-changed=WIT_WORLD");
    // the PROFILE env var unfortunately only gives us 1 bit of "dev or release"
    let cargo_profile = if profile == "debug" { "dev" } else { "release" };

    let mut cargo_cmd = cargo_hyperlight::cargo().unwrap();
    let mut cmd = cargo_cmd
        .arg("build")
        .arg("--profile")
        .arg(cargo_profile)
        .arg("-v")
        .arg("--target-dir")
        .arg(&target_dir)
        .arg("--manifest-path")
        .arg(&manifest_path)
        .arg("--locked")
        .env_clear_cargo();

    // Add --features gdb if the gdb feature is enabled for this build script
    if std::env::var("CARGO_FEATURE_GDB").is_ok() {
        cmd = cmd.arg("--features").arg("gdb");
    }
    // Add --features pulley if the pulley feature is enabled
    if std::env::var("CARGO_FEATURE_PULLEY").is_ok() {
        cmd = cmd.arg("--features").arg("pulley");
    }
    // Enable the "trace_guest" feature if the corresponding Cargo feature is enabled
    if std::env::var("CARGO_FEATURE_TRACE_GUEST").is_ok() {
        cmd = cmd.arg("--features").arg("trace_guest");
    }

    cmd.status()
        .unwrap_or_else(|e| panic!("could not run cargo build hyperlight-wasm-runtime: {e:?}"));

    let resource = target_dir
        .join("x86_64-hyperlight-none")
        .join(profile)
        .join("hyperlight-wasm-runtime");

    if let Ok(path) = resource.canonicalize() {
        if std::env::var("CARGO_FEATURE_GDB").is_ok() {
            println!(
                "cargo:warning=Hyperlight wasm runtime guest binary at: {}",
                path.display()
            );
        }

        path
    } else {
        panic!(
            "could not find hyperlight-wasm-runtime after building it (expected {:?})",
            resource
        )
    }
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

    // get the wasmtime version number from the hyperlight-wasm-runtime metadata

    let wasm_runtime_bytes = fs::read(&wasm_runtime_resource).unwrap();
    let elf = goblin::elf::Elf::parse(&wasm_runtime_bytes).unwrap();

    // the hyperlight-wasm-runtime binary has a section named .note_hyperlight_metadata that contains the wasmtime version number
    // this section is added to the hyperlight-wasm-runtime binary by the build.rs script in the hyperlight-wasm-runtime crate
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
        panic!(".note_hyperlight_metadata section not found in hyperlight-wasm-runtime binary");
    };

    // write the build information to the built.rs file
    write_built_file()?;

    // open the built.rs file and append the details of the hyperlight-wasm-runtime file
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

    // Calculate the blake3 hash of the hyperlight-wasm-runtime file and write it to the wasm_runtime_resource.rs file so we can include it in the binary
    let hyperlight_wasm_runtime = fs::read(wasm_runtime_resource).unwrap();
    let hash = blake3::hash(&hyperlight_wasm_runtime);
    let hash_str = format!("static WASM_RUNTIME_BLAKE3_HASH: &str = \"{}\";", hash);

    writeln!(file, "{}", hash_str).unwrap();

    println!("cargo:rerun-if-changed=build.rs");

    cfg_aliases::cfg_aliases! {
        gdb: { all(feature = "gdb", debug_assertions) },
    }

    Ok(())
}
