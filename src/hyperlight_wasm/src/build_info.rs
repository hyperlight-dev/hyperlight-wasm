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

use std::sync::Once;

use log::info;
// LOG_ONCE is used to log information about the crate version once
static LOG_ONCE: Once = Once::new();

// The `built` crate is used in build.rs to generate a `built.rs` file that contains
// information about the build environment.
//
// In addition build.rs appends information about the wasm_runtime binary to the `built.rs` file
//
// Collectively that information is used to populate the `BuildInfo` struct.
//
include!(concat!(env!("OUT_DIR"), "/built.rs"));

/// The build information for the hyperligt-wasm crate
pub struct BuildInfo {
    /// The date and time the wasm_runtime was built
    pub wasm_runtime_created: &'static str,
    /// The size of the wasm_runtime binary
    pub wasm_runtime_size: &'static str,
    /// The blake3 hash of the wasm_runtime binary
    pub wasm_runtime_blake3_hash: &'static str,
    /// The version of wasmtime being used by hyperlight-wasm
    pub wasm_runtime_wasmtime_version: &'static str,
    /// The name of the package
    pub package_name: &'static str,
    /// The version of the package
    pub package_version: &'static str,
    /// The features enabled for the package
    pub features: Vec<&'static str>,
    /// The target triple for the build
    pub target: &'static str,
    /// The optimization level for the build
    pub opt_level: &'static str,
    /// The profile for the build
    pub profile: &'static str,
    /// Whether the build was in debug mode
    pub debug: bool,
    /// The path to rustc used for the build
    pub rustc: &'static str,
    /// The date and time the package was built
    pub built_time_utc: &'static str,
    /// The CI platform used for the build
    pub ci_platform: Option<&'static str>,
    /// The git commit hash for the build
    pub git_commit_hash: Option<&'static str>,
    /// The git head ref for the build
    pub git_head_ref: Option<&'static str>,
    /// The git version for the build
    pub git_version: Option<&'static str>,
    /// Whether the git repo was dirty when the package was built
    pub git_dirty: bool,
}

impl Default for BuildInfo {
    fn default() -> Self {
        let mut features: Vec<&str> = Vec::new();

        for feature in FEATURES {
            features.push(feature);
        }

        Self {
            wasm_runtime_created: WASM_RUNTIME_CREATED,
            wasm_runtime_size: WASM_RUNTIME_SIZE,
            wasm_runtime_blake3_hash: WASM_RUNTIME_BLAKE3_HASH,
            wasm_runtime_wasmtime_version: WASM_RUNTIME_WASMTIME_VERSION,
            package_name: PKG_NAME,
            package_version: PKG_VERSION,
            features,
            target: TARGET,
            opt_level: OPT_LEVEL,
            profile: PROFILE,
            debug: DEBUG,
            rustc: RUSTC,
            built_time_utc: BUILT_TIME_UTC,
            ci_platform: CI_PLATFORM,
            git_commit_hash: GIT_COMMIT_HASH,
            git_head_ref: GIT_HEAD_REF,
            git_version: GIT_VERSION,
            git_dirty: GIT_DIRTY.unwrap_or(false),
        }
    }
}

impl BuildInfo {
    /// Get the build information
    pub fn get() -> Self {
        Self::default()
    }
    pub(crate) fn log() {
        Self::default().log_build_details();
    }
    fn log_build_details(&self) {
        LOG_ONCE.call_once(|| {
            info!("{}", self);
        });
    }
    /// Get the version of wasmtime being used by hyperlight-wasm
    pub(crate) fn get_wasmtime_version() -> &'static str {
        WASM_RUNTIME_WASMTIME_VERSION
    }
}

impl std::fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "wasm_runtime created at: {}", self.wasm_runtime_created)?;
        writeln!(f, "wasm_runtime size: {}", self.wasm_runtime_size)?;
        writeln!(f, "wasm_runtime hash: {}", self.wasm_runtime_blake3_hash)?;
        writeln!(
            f,
            "wasm_runtime wasmtime version: {}",
            self.wasm_runtime_wasmtime_version
        )?;
        writeln!(f, "Package name: {}", self.package_name)?;
        writeln!(f, "Package version: {}", self.package_version)?;
        writeln!(f, "Package features: {:#?}", self.features)?;
        writeln!(f, "Target triple: {}", self.target)?;
        writeln!(f, "Optimization level: {}", self.opt_level)?;
        writeln!(f, "Profile: {}", self.profile)?;
        writeln!(f, "Debug: {}", self.debug)?;
        writeln!(f, "Rustc: {}", self.rustc)?;
        writeln!(f, "Built at: {}", self.built_time_utc)?;
        writeln!(f, "CI platform: {:?}", self.ci_platform.unwrap_or("None"))?;
        writeln!(
            f,
            "Git commit hash: {:?}",
            self.git_commit_hash.unwrap_or("None")
        )?;
        writeln!(f, "Git head ref: {:?}", self.git_head_ref.unwrap_or("None"))?;
        writeln!(f, "Git version: {:?}", self.git_version.unwrap_or("None"))?;
        if self.git_dirty {
            writeln!(f, "Repo had uncommitted changes when built")?;
        }
        Ok(())
    }
}
