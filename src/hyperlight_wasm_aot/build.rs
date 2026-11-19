// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Hyperlight Authors.

// Records where `experimental-build-runtime` finds the hyperlight-wasm-runtime
// source, in OUT_DIR/runtime_source.rs.

use std::path::PathBuf;
use std::{env, fs};

use cargo_metadata::MetadataCommand;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    let metadata = MetadataCommand::new()
        .exec()
        .expect("could not run cargo metadata");
    let runtime = metadata
        .packages
        .iter()
        .find(|p| *p.name == "hyperlight-wasm-runtime")
        .expect("hyperlight-wasm-runtime not found in cargo metadata");
    let manifest = runtime.manifest_path.as_std_path();
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let registry = runtime
        .source
        .as_ref()
        .is_some_and(|s| s.repr.starts_with("registry+") || s.repr.starts_with("sparse+"));
    let source = if !registry {
        // A checkout, including a git one, builds the runtime in place, against
        // its workspace's other crates. A packaged copy would use their released
        // versions.
        format!("RuntimeSource::Path({manifest:?})")
    } else {
        // Cargo can garbage collect registry sources, so embed them.
        let prefix = format!("hyperlight-wasm-runtime-{}", runtime.version);
        let archive = out_dir.join("runtime-source.tar");
        let mut builder = tar::Builder::new(fs::File::create(&archive).unwrap());
        builder
            .append_dir_all(&prefix, manifest.parent().unwrap())
            .unwrap();
        builder.finish().unwrap();
        format!("RuntimeSource::Archive({prefix:?}, include_bytes!({archive:?}))")
    };
    fs::write(
        out_dir.join("runtime_source.rs"),
        format!("const RUNTIME_SOURCE: RuntimeSource = {source};\n"),
    )
    .unwrap();
}
