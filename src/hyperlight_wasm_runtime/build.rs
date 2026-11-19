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

use std::io::Write;
use std::path::{Path, PathBuf};
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
    fs::write(&dest_path, file_contents).unwrap();

    cfg.include("src/include");
    cfg.file("src/platform.c");
    if cfg!(windows) {
        env::set_var("AR_x86_64_unknown_none", "llvm-ar");
    }
    cfg.compile("wasmtime-hyperlight-platform");

    println!("cargo::rerun-if-env-changed=WIT_WORLD");
    println!("cargo::rerun-if-env-changed=WIT_WORLD_NAME");
    println!("cargo::rustc-check-cfg=cfg(component)");
    let interface = env::var_os("WIT_WORLD").map(|path| {
        let path = Path::new(&env::var_os("CARGO_MANIFEST_DIR").unwrap()).join(path);
        println!("cargo::rerun-if-changed={}", path.display());
        println!("cargo::rustc-cfg=component");
        component_interface(&path)
    });
    // The host reads this to check the runtime's version and world.
    let identity = format!(
        "{}\n{}",
        env::var("CARGO_PKG_VERSION").unwrap(),
        interface.unwrap_or_default()
    );
    let mut metadata = fs::OpenOptions::new()
        .append(true)
        .open(&dest_path)
        .unwrap();
    write!(
        metadata,
        r#"
    #[used]
    #[link_section = ".note_hyperlight_wasm_runtime"]
    static RUNTIME_IDENTITY: [u8; {len}] = *b"{identity}";
    "#,
        len = identity.len(),
        identity = identity.escape_default(),
    )
    .unwrap();

    cfg_aliases::cfg_aliases! {
        gdb: { all(feature = "gdb", debug_assertions) },
        pulley: { feature = "pulley" },
    }
}

/// Identifies the `WIT_WORLD` world, so hosts can check that a snapshot
/// matches their bindings. Hashes the file, or the files a directory's WIT
/// package is read from, and `WIT_WORLD_NAME`.
fn component_interface(path: &Path) -> String {
    let mut hash = blake3::Hasher::new();
    if path.is_dir() {
        hash.update(b"\x02");
        hash_package_dir(&mut hash, path, true);
    } else {
        hash_file(&mut hash, path);
    }
    // One file can hold several worlds. Unset selects the last one.
    if let Ok(world) = env::var("WIT_WORLD_NAME") {
        hash.update(b"\x01");
        hash.update(world.as_bytes());
    }
    hash.finalize().to_string()
}

fn hash_file(hash: &mut blake3::Hasher, path: &Path) {
    let bytes = fs::read(path).expect("could not read WIT_WORLD");
    hash.update(blake3::hash(&bytes).as_bytes());
}

fn hash_name(hash: &mut blake3::Hasher, name: &str) {
    hash.update(&(name.len() as u64).to_le_bytes());
    hash.update(name.as_bytes());
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(dir)
        .and_then(|d| {
            d.map(|e| e.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()
        })
        .expect("could not read WIT_WORLD");
    entries.sort();
    entries
}

/// Hashes the files wit-parser reads for a package directory: its `*.wit`
/// files and, at the top level, the packages in `deps`.
fn hash_package_dir(hash: &mut blake3::Hasher, dir: &Path, top_level: bool) {
    for entry in sorted_entries(dir) {
        let name = entry.file_name().unwrap().to_string_lossy();
        if !entry.is_dir() && name.ends_with(".wit") {
            hash_name(hash, &name);
            hash_file(hash, &entry);
        }
    }
    let deps = dir.join("deps");
    if !top_level || !deps.is_dir() {
        return;
    }
    hash.update(b"\x03");
    for entry in sorted_entries(&deps) {
        let name = entry.file_name().unwrap().to_string_lossy();
        if entry.is_dir() {
            hash_name(hash, &format!("{name}/"));
            hash_package_dir(hash, &entry, false);
        } else if matches!(
            entry.extension().and_then(|e| e.to_str()),
            Some("wit" | "wat" | "wasm")
        ) {
            hash_name(hash, &name);
            hash_file(hash, &entry);
        }
    }
}
