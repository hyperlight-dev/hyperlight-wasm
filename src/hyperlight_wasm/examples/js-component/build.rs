use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use componentize_qjs::{ComponentizeOpts, Runtime};
use wasmtime_aot::{Config as AotConfig, Engine as AotEngine};

fn main() -> Result<()> {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let wit_path = manifest_dir.join("wit").join("hello.wit");
    let wit_world_path = manifest_dir.join("wit").join("hello-world.wasm");
    let js_path = manifest_dir.join("js").join("hello.js");

    println!("cargo:rerun-if-changed={}", wit_path.display());
    println!("cargo:rerun-if-changed={}", wit_world_path.display());
    println!("cargo:rerun-if-changed={}", js_path.display());

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let component_path = out_dir.join("hello.component.wasm");
    let aot_path = out_dir.join("hello.component.aot");

    let js_source = fs::read_to_string(&js_path)
        .with_context(|| format!("failed to read {}", js_path.display()))?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    let component = runtime
        .block_on(componentize_qjs::componentize(&ComponentizeOpts {
            wit_path: &wit_path,
            js_source: &js_source,
            js_path: Some(&js_path),
            module_root: Some(&manifest_dir),
            world_name: Some("hello"),
            stub_wasi: true,
            disable_gc: false,
            runtime: Runtime::DefaultSync,
        }))
        .context("failed to build JavaScript component")?;

    fs::write(&component_path, &component)
        .with_context(|| format!("failed to write {}", component_path.display()))?;

    let mut config = AotConfig::new();
    config.target("x86_64-unknown-none")?;

    let engine = AotEngine::new(&config)?;
    let aot = engine
        .precompile_component(&component)
        .context("failed to AOT-compile JavaScript component")?;

    fs::write(&aot_path, aot).with_context(|| format!("failed to write {}", aot_path.display()))?;
    println!("cargo:rustc-env=QJS_COMPONENT_AOT={}", aot_path.display());

    Ok(())
}
