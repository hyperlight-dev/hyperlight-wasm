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

use std::env;
use std::path::Path;

use hyperlight_host::{new_error, Result};

/// Get the Wasm module called `name` from the standard module location
/// at `$REPO_ROOT/x64/[debug|release]/$name`.
pub fn get_wasm_module_path(name: &str) -> Result<String> {
    #[cfg(debug_assertions)]
    let config = "debug";
    #[cfg(not(debug_assertions))]
    let config = "release";

    let proj_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| {
        env::var_os("RUST_DIR_FOR_DEBUGGING_TESTS")
            .expect("Failed to get CARGO_MANIFEST_DIR  or RUST_DIR_FOR_DEBUGGING_TESTS env var")
    });

    let x64_dir = Path::new(&proj_dir)
        .join("../../x64")
        .join(config)
        .join(name);

    let path = match x64_dir.canonicalize() {
        Ok(path) => path.to_str().unwrap().to_string(),
        Err(e) => {
            return Err(new_error!(
                "Failed to get the path to the Wasm module: {:?} {}",
                x64_dir,
                e,
            ));
        }
    };
    Ok(path)
}
