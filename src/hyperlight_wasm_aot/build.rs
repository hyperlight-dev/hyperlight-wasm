use cargo_metadata::MetadataCommand;
use cargo_util_schemas::manifest::PackageName;

fn main() {
    let metadata = MetadataCommand::new().exec().unwrap();
    let this_pkg_name = PackageName::new("hyperlight-wasm-aot").unwrap();
    let this_pkg_id = metadata
        .workspace_members
        .iter()
        .find(|x| metadata[x].name == this_pkg_name)
        .unwrap();
    let this_pkg = &metadata.resolve.as_ref().unwrap()[this_pkg_id];
    let get_dep = |name| {
        let mut iter = this_pkg.deps.iter().filter(|x| x.name == name);
        let dep = iter
            .next()
            .unwrap_or_else(|| panic!("hyperlight-wasm-aot has no {name} dependency"));
        if iter.next().is_some() {
            panic!("hyperlight-wasm-aot has multiple {name} dependencies");
        }
        dep
    };
    let wasmtime_lts = &metadata[&get_dep("wasmtime_lts").pkg];
    let wasmtime_latest = &metadata[&get_dep("wasmtime").pkg];
    println!(
        "cargo::rustc-env=WASMTIME_LTS_VERSION={}",
        wasmtime_lts.version
    );
    println!(
        "cargo::rustc-env=WASMTIME_LATEST_VERSION={}",
        wasmtime_latest.version
    );
}
