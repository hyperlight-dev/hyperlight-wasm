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

fn get_wasm_runtime_path() -> PathBuf {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir = PathBuf::from(manifest_dir);

    let tar_path = manifest_dir.join("vendor.tar");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let out_dir = PathBuf::from(out_dir);
    let vendor_dir = out_dir.join("vendor");

    if vendor_dir.exists() {
        fs::remove_dir_all(&vendor_dir).unwrap();
    }

    println!("cargo::rerun-if-changed={}", tar_path.display());

    // If the vendor.tar file exists, extract it to the OUT_DIR/vendor directory
    // and return the wasm_runtime directory inside it.
    // This is useful for vendoring the wasm_runtime crate in a release build, since crates.io
    // does not allow vendoring folders with Cargo.toml files (i.e., other crates).
    // The vendor.tar file is expected to be in the same directory as this build script.
    if tar_path.exists() {
        let out_dir = env::var_os("OUT_DIR").unwrap();
        let out_dir = PathBuf::from(out_dir);
        let vendor_dir = out_dir.join("vendor");

        let mut tar = tar::Archive::new(fs::File::open(&tar_path).unwrap());
        tar.unpack(&vendor_dir).unwrap();

        let wasm_runtime_dir = vendor_dir.join("wasm_runtime");

        println!(
            "cargo::warning=using vendor wasm_runtime from {}",
            tar_path.display()
        );
        return wasm_runtime_dir;
    }

    let crates_dir = manifest_dir.parent().unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(crates_dir, &vendor_dir).unwrap();

    #[cfg(not(unix))]
    junction::create(crates_dir, &vendor_dir).unwrap();

    let wasm_runtime_dir = crates_dir.join("wasm_runtime");
    if wasm_runtime_dir.exists() {
        return wasm_runtime_dir;
    }

    panic!(
        r#"
        The wasm_runtime directory not found in the expected locations.
        If you are using hyperlight-wasm from a crates.io release, please file an issue: https://github.com/hyperlight-dev/hyperlight-wasm/issues
        "#
    );
}

fn build_wasm_runtime() -> PathBuf {
    let profile = env::var_os("PROFILE").unwrap();
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let target_dir = Path::new("").join(&out_dir).join("target");

    let in_repo_dir = get_wasm_runtime_path();

    if !in_repo_dir.exists() {
        panic!("missing wasm_runtime in-tree dependency");
    }

    println!("cargo::rerun-if-changed={}", in_repo_dir.display());
    println!("cargo::rerun-if-env-changed=WIT_WORLD");
    // the PROFILE env var unfortunately only gives us 1 bit of "dev or release"
    let cargo_profile = if profile == "debug" { "dev" } else { "release" };

    // Clear the variables that control Cargo's behaviour (as listed
    // at https://doc.rust-lang.org/cargo/reference/environment-variables.html):
    // otherwise the nested build will build the wrong thing
    let mut env_vars = env::vars().collect::<Vec<_>>();
    env_vars.retain(|(key, _)| !key.starts_with("CARGO_"));

    let mut cargo_cmd = cargo_hyperlight::cargo().unwrap();
    let mut cmd = cargo_cmd
        .arg("build")
        .arg("--target-dir")
        .arg(&target_dir)
        .arg("--profile")
        .arg(cargo_profile)
        .arg("-v")
        .current_dir(&in_repo_dir)
        .env_clear()
        .envs(env_vars);

    // Use wasmtime LTS version
    // This removes wasmtime_latest feature and adds wasmtime_lts
    if std::env::var("CARGO_FEATURE_WASMTIME_LTS").is_ok() {
        cmd = cmd
            .arg("--no-default-features")
            .arg("--features")
            .arg("wasmtime_lts");
    }

    // Add --features gdb if the gdb feature is enabled for this build script
    if std::env::var("CARGO_FEATURE_GDB").is_ok() {
        cmd = cmd.arg("--features").arg("gdb");
    }
    // Enable the "trace_guest" feature if the corresponding Cargo feature is enabled
    if std::env::var("CARGO_FEATURE_TRACE_GUEST").is_ok() {
        cmd = cmd.arg("--features").arg("trace_guest");
    }

    cmd.status()
        .unwrap_or_else(|e| panic!("could not run cargo build wasm_runtime: {}", e));

    let resource = target_dir
        .join("x86_64-hyperlight-none")
        .join(profile)
        .join("wasm_runtime");

    if let Ok(path) = resource.canonicalize() {
        if std::env::var("CARGO_FEATURE_GDB").is_ok() {
            println!(
                "cargo:warning=Wasm runtime guest binary at: {}",
                path.display()
            );
        }

        path
    } else {
        panic!(
            "could not find wasm_runtime after building it (expected {:?})",
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

    cfg_aliases::cfg_aliases! {
        gdb: { all(feature = "gdb", debug_assertions) },
    }

    Ok(())
}
