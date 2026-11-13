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

use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use cargo_metadata::{MetadataCommand, Package};
use cargo_util_schemas::manifest::PackageName;
use clap::{Parser, Subcommand, ValueEnum};
use object::read::elf::ElfFile64;
use object::{Architecture, Endianness, FileFlags, Object};
use wasmtime::{Config, Engine, Module, OptLevel, Precompiled};

#[derive(Debug)]
enum SupportedTarget {
    X86_64UnknownNone,
    AArch64UnknownNone,
    WasmtimePulley64,
}

impl Display for SupportedTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupportedTarget::X86_64UnknownNone => write!(f, "x86_64-unknown-none"),
            SupportedTarget::AArch64UnknownNone => write!(f, "aarch64-unknown-none"),
            SupportedTarget::WasmtimePulley64 => write!(f, "pulley64"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum WasmtimeVersion {
    Lts,
    Latest,
}

#[derive(Parser)]
#[command(name = "hyperlight-wasm-aot")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Precompile WebAssembly modules and components for hyperlight-wasm")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Precompile a WebAssembly module or component for Wasmtime
    Compile {
        /// The input WebAssembly file
        input: String,

        /// The output file path (defaults to input with .aot extension)
        output: Option<String>,

        /// Compile a component rather than a module
        #[arg(long)]
        component: bool,

        /// Precompile with debug and disable optimizations
        #[arg(long)]
        debug: bool,

        /// Disable address map and native unwind info for smaller binaries
        #[arg(long)]
        minimal: bool,

        /// Pre-compile for the pulley64 target
        #[arg(long)]
        pulley: bool,

        /// Wasmtime version used for precompilation
        #[arg(long, value_enum, default_value = "lts")]
        wasmtime_version: WasmtimeVersion,
    },

    /// Check which Wasmtime version was used to precompile a module
    CheckWasmtimeVersion {
        /// The precompiled file to check
        file: String,

        /// Specifies if the module has been compiled with debug support
        #[arg(long)]
        debug: bool,

        /// Wasmtime version used to validate the precompiled module
        #[arg(long, value_enum, default_value = "lts")]
        wasmtime_version: WasmtimeVersion,
    },

    /// Experimental: build a guest runtime for a WIT world
    ///
    /// Needs rustup and clang. Builds with Rust 1.94.1 unless RUSTUP_TOOLCHAIN is
    /// set. Use a runtime with hosts and precompiled components from this version
    /// of hyperlight-wasm.
    ExperimentalBuildRuntime {
        /// The `.wit` file or wasm-encoded WIT package that defines the world
        wit: PathBuf,

        /// The output file path
        output: PathBuf,

        /// The world to build for. Defaults to the last world in the package.
        #[arg(long)]
        world: Option<String>,

        /// Build with debug info and optimizations disabled
        #[arg(long)]
        debug: bool,

        /// Wasmtime version the runtime uses. Precompile components with the same version.
        #[arg(long, value_enum, default_value = "lts")]
        wasmtime_version: WasmtimeVersion,

        /// Directory for build artifacts, reused by later builds
        #[arg(long, default_value = "target/hyperlight-wasm-runtime")]
        target_dir: PathBuf,
    },
}

// Only one variant is built, chosen by build.rs.
#[allow(dead_code)]
enum RuntimeSource {
    /// The runtime's manifest in a checkout.
    Path(&'static str),
    /// The top-level directory name and a tar of the runtime crate.
    Archive(&'static str, &'static [u8]),
}

include!(concat!(env!("OUT_DIR"), "/runtime_source.rs"));

type BoxError = Box<dyn std::error::Error>;

/// The toolchain that builds the runtime when `RUSTUP_TOOLCHAIN` is unset.
/// Keep it in sync with rust-toolchain.toml.
const RUNTIME_TOOLCHAIN: &str = "1.94.1";

/// Returns the runtime's manifest, extracting an embedded source into `target_dir`.
fn runtime_manifest(target_dir: &Path) -> Result<PathBuf, BoxError> {
    match RUNTIME_SOURCE {
        RuntimeSource::Path(manifest) => Ok(PathBuf::from(manifest)),
        RuntimeSource::Archive(prefix, archive) => {
            let parent = target_dir.join("source");
            let source = parent.join(prefix);
            // The marker shows that an earlier extraction finished.
            let marker = source.join(".hyperlight-wasm-aot-extracted");
            if !marker.exists() {
                if source.exists() {
                    fs::remove_dir_all(&source)?;
                }
                tar::Archive::new(archive).unpack(&parent)?;
                // Keep Cargo from treating an enclosing project as the workspace.
                let manifest = source.join("Cargo.toml");
                let mut contents = fs::read_to_string(&manifest)?;
                contents.push_str("\n[workspace]\n");
                fs::write(&manifest, contents)?;
                fs::write(&marker, "")?;
            }
            Ok(source.join("Cargo.toml"))
        }
    }
}

fn build_runtime(
    wit: &Path,
    output: &Path,
    world: Option<&str>,
    debug: bool,
    wasmtime_version: WasmtimeVersion,
    target_dir: &Path,
) -> Result<(), BoxError> {
    // The runtime resolves a relative WIT_WORLD against its own manifest.
    let wit = wit
        .canonicalize()
        .map_err(|e| format!("could not read {}: {e}", wit.display()))?;
    let manifest = runtime_manifest(target_dir)?;
    let target = format!("{}-hyperlight-none", std::env::consts::ARCH);
    let (profile, profile_dir) = if debug {
        ("dev", "debug")
    } else {
        ("release", "release")
    };

    // Stable Rust 1.95 and later reject the guest's custom target. cargo-hyperlight
    // also adds rust-src only when RUSTUP_TOOLCHAIN is set in this process.
    if std::env::var_os("RUSTUP_TOOLCHAIN").is_none() {
        // SAFETY: the process has not started other threads.
        unsafe { std::env::set_var("RUSTUP_TOOLCHAIN", RUNTIME_TOOLCHAIN) };
    }

    let mut cmd = cargo_hyperlight::cargo()?;
    cmd.env_clear_cargo()
        .arg("build")
        .arg("--profile")
        .arg(profile)
        .arg("--target")
        .arg(&target)
        .arg("--target-dir")
        .arg(target_dir)
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--locked")
        .env("WIT_WORLD", &wit);
    match world {
        Some(world) => cmd.env("WIT_WORLD_NAME", world),
        None => cmd.env_remove("WIT_WORLD_NAME"),
    };
    if wasmtime_version == WasmtimeVersion::Latest {
        cmd.args(["--no-default-features", "--features", "wasmtime_latest"]);
    }
    cmd.status()?;

    let built = target_dir
        .join(&target)
        .join(profile_dir)
        .join("hyperlight-wasm-runtime");
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&built, output).map_err(|e| {
        format!(
            "could not copy {} to {}: {e}",
            built.display(),
            output.display()
        )
    })?;
    println!(
        "Built guest runtime for {}: {}",
        wit.display(),
        output.display()
    );
    Ok(())
}

/// Precompile bytes using the LTS wasmtime version
fn precompile_bytes_lts(
    bytes: &[u8],
    debug: bool,
    minimal: bool,
    pulley: bool,
    is_component: bool,
) -> Vec<u8> {
    let mut config = wasmtime_lts::Config::new();
    if pulley {
        config.target("pulley64").unwrap();
    } else {
        #[cfg(target_arch = "x86_64")]
        config.target("x86_64-unknown-none").unwrap();
        #[cfg(target_arch = "aarch64")]
        config.target("aarch64-unknown-none").unwrap();
    }
    if debug {
        config.debug_info(true);
        config.cranelift_opt_level(wasmtime_lts::OptLevel::None);
    }
    if minimal {
        config.generate_address_map(false);
        config.native_unwind_info(false);
    }
    let engine = wasmtime_lts::Engine::new(&config).unwrap();
    if is_component {
        engine.precompile_component(bytes).unwrap()
    } else {
        engine.precompile_module(bytes).unwrap()
    }
}

/// Detect and deserialize using the LTS wasmtime version
fn detect_and_deserialize_lts(bytes: &[u8], debug: bool, file: &str) {
    let mut config = wasmtime_lts::Config::new();
    #[cfg(target_arch = "x86_64")]
    config.target("x86_64-unknown-none").unwrap();
    #[cfg(target_arch = "aarch64")]
    config.target("aarch64-unknown-none").unwrap();
    if debug {
        config.debug_info(true);
        config.cranelift_opt_level(wasmtime_lts::OptLevel::None);
    }
    let engine = wasmtime_lts::Engine::new(&config).unwrap();
    match wasmtime_lts::Engine::detect_precompiled(bytes) {
        Some(wasmtime_lts::Precompiled::Module) => {
            println!("The file is a valid AOT compiled Wasmtime module");
            match unsafe { wasmtime_lts::Module::deserialize(&engine, bytes) } {
                Ok(_) => println!(
                    "File {} was AOT compiled with a compatible wasmtime version (LTS)",
                    file
                ),
                Err(e) => eprintln!("{}", e),
            }
        }
        Some(wasmtime_lts::Precompiled::Component) => {
            println!("The file is an AOT compiled Wasmtime component")
        }
        None => {
            eprintln!(
                "Error - {} is not a valid AOT compiled Wasmtime module or component",
                file
            );
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            input,
            output,
            component,
            debug,
            minimal,
            pulley,
            wasmtime_version,
        } => {
            let outfile = match output {
                Some(s) => s,
                None => {
                    let mut path = Path::new(&input).to_path_buf();
                    path.set_extension("aot");
                    path.to_str().unwrap().to_string()
                }
            };

            match wasmtime_version {
                WasmtimeVersion::Latest => {
                    let target = if pulley {
                        SupportedTarget::WasmtimePulley64
                    } else {
                        #[cfg(target_arch = "x86_64")]
                        let target = SupportedTarget::X86_64UnknownNone;
                        #[cfg(target_arch = "aarch64")]
                        let target = SupportedTarget::AArch64UnknownNone;
                        target
                    };
                    if debug {
                        println!(
                            "Aot Compiling {} to [{}]: {} with debug info and optimizations off (latest wasmtime)",
                            input, target, outfile
                        );
                    } else {
                        println!(
                            "Aot Compiling {} to [{}]: {} (latest wasmtime)",
                            input, target, outfile
                        );
                    }
                    let config = get_config(debug, minimal, &target);
                    let engine = Engine::new(&config).unwrap();
                    let bytes = std::fs::read(&input).unwrap();
                    let serialized = if component {
                        engine.precompile_component(&bytes).unwrap()
                    } else {
                        engine.precompile_module(&bytes).unwrap()
                    };
                    std::fs::write(outfile, serialized).unwrap();
                }
                WasmtimeVersion::Lts => {
                    let target_name = if pulley {
                        "pulley64"
                    } else {
                        #[cfg(target_arch = "x86_64")]
                        {
                            "x86_64-unknown-none"
                        }
                        #[cfg(target_arch = "aarch64")]
                        {
                            "aarch64-unknown-none"
                        }
                    };
                    if debug {
                        println!(
                            "Aot Compiling {} to [{}]: {} with debug info and optimizations off (LTS wasmtime)",
                            input, target_name, outfile
                        );
                    } else {
                        println!(
                            "Aot Compiling {} to [{}]: {} (LTS wasmtime)",
                            input, target_name, outfile
                        );
                    }
                    let bytes = std::fs::read(&input).unwrap();
                    let serialized =
                        precompile_bytes_lts(&bytes, debug, minimal, pulley, component);
                    std::fs::write(outfile, serialized).unwrap();
                }
            }
        }
        Commands::CheckWasmtimeVersion {
            file,
            debug,
            wasmtime_version,
        } => {
            match wasmtime_version {
                WasmtimeVersion::Latest => {
                    // get the wasmtime version used by hyperlight-wasm-aot
                    let metadata = MetadataCommand::new().exec().unwrap();
                    let package_name = PackageName::new("wasmtime".to_string()).unwrap();
                    let wasmtime_package: Option<&Package> = metadata
                        .packages
                        .iter()
                        .filter(|p| p.name == package_name)
                        .find(|p| p.version.major > 36);
                    let version_number = match wasmtime_package {
                        Some(pkg) => pkg.version.clone(),
                        None => panic!("latest wasmtime dependency not found"),
                    };
                    if debug {
                        println!(
                            "Checking Wasmtime version used to compile debug info enabled file: {} (latest wasmtime)",
                            file
                        );
                    } else {
                        println!(
                            "Checking Wasmtime version used to compile file: {} (latest wasmtime)",
                            file
                        );
                    }
                    let bytes = std::fs::read(&file).unwrap();
                    let target = match get_aot_target(&bytes) {
                        Ok(target) => target,
                        Err(e) => {
                            eprintln!(
                                "Error - {} is not a valid precompiled Wasmtime module: {}",
                                file, e
                            );
                            std::process::exit(1)
                        }
                    };
                    let config = get_config(debug, false, &target);
                    let engine = Engine::new(&config).unwrap();
                    match Engine::detect_precompiled(&bytes) {
                        Some(pre_compiled) => {
                            match pre_compiled {
                                Precompiled::Module => {
                                    println!("The file is a valid AOT compiled Wasmtime module");
                                    // It doesnt seem like the functions or data needed to extract the version of wasmtime used to compile the module are exposed in the wasmtime crate
                                    // so we will try and load it and then catch the error and parse the version from the error message :-(
                                    match unsafe { Module::deserialize(&engine, bytes) } {
                                        Ok(_) => println!(
                                            "File {} was AOT compiled to '{}' with wasmtime version: {}",
                                            file, target, version_number
                                        ),
                                        Err(e) => {
                                            let error_message = e.to_string();
                                            if !error_message.starts_with(
                                                "Module was compiled with incompatible Wasmtime version",
                                            ) {
                                                eprintln!("{}", error_message);
                                                return;
                                            }
                                            let version = error_message.trim_start_matches("Module was compiled with incompatible Wasmtime version ").trim();
                                            println!(
                                                "File {} was AOT compiled to '{}' with wasmtime version: {}",
                                                file, target, version
                                            );
                                        }
                                    };
                                }
                                Precompiled::Component => {
                                    eprintln!("The file is an AOT compiled Wasmtime component")
                                }
                            }
                        }
                        None => {
                            eprintln!(
                                "Error - {} is not a valid AOT compiled Wasmtime module or component",
                                file
                            );
                        }
                    };
                }
                WasmtimeVersion::Lts => {
                    if debug {
                        println!(
                            "Checking Wasmtime version used to compile with debug info enabled file: {} (LTS wasmtime)",
                            file
                        );
                    } else {
                        println!(
                            "Checking Wasmtime version used to compile file: {} (LTS wasmtime)",
                            file
                        );
                    }
                    let bytes = std::fs::read(&file).unwrap();
                    detect_and_deserialize_lts(&bytes, debug, &file);
                }
            }
        }
        Commands::ExperimentalBuildRuntime {
            wit,
            output,
            world,
            debug,
            wasmtime_version,
            target_dir,
        } => {
            if let Err(e) = build_runtime(
                &wit,
                &output,
                world.as_deref(),
                debug,
                wasmtime_version,
                &target_dir,
            ) {
                eprintln!("Error - {e}");
                std::process::exit(1);
            }
        }
    }
}

/// Returns a new `Config` for the Wasmtime engine with additional settings for AOT compilation.
fn get_config(debug: bool, minimal: bool, target: &SupportedTarget) -> Config {
    let mut config = Config::new();

    // Compile for the pulley64 target if specified
    match target {
        SupportedTarget::X86_64UnknownNone => {
            config.target("x86_64-unknown-none").unwrap();
            // Enable x86_float_abi_ok only for the latest Wasmtime version.
            // Safety:
            // We are using hyperlight cargo to build the guest which
            // sets the Rust target to be compiled with the hard-float ABI manually via
            // `-Zbuild-std` and a custom target JSON configuration
            // See https://github.com/bytecodealliance/wasmtime/pull/11553
            unsafe { config.x86_float_abi_ok(true) };
        }
        SupportedTarget::AArch64UnknownNone => {
            config.target("aarch64-unknown-none").unwrap();
        }
        SupportedTarget::WasmtimePulley64 => {
            config.target("pulley64").unwrap();
        }
    }

    // Enable the default features for the Wasmtime engine.
    if debug {
        config.debug_info(true);
        config.cranelift_opt_level(OptLevel::None);
    }

    if minimal {
        config.generate_address_map(false);
        config.native_unwind_info(false);
    }

    config
}

/// Parses the AOT compiled file as an ELF file and extracts the target triple
/// NOTE: These flag bits must match Wasmtime's EF_WASMTIME_PULLEY{64,32} values
/// used when emitting RISC-V ELF object files. If Wasmtime changes these values,
/// this code must be updated accordingly to correctly detect pulley targets.
/// Source of definitions:
/// https://github.com/bytecodealliance/wasmtime/blob/release-42.0.0/crates/environ/src/obj.rs#L26
/// Source of logic for detecting pulley targets:
/// https://github.com/bytecodealliance/wasmtime/blob/release-42.0.0/src/commands/objdump.rs#L408
fn get_aot_target(bytes: &[u8]) -> Result<SupportedTarget, String> {
    const EF_WASMTIME_PULLEY64: u32 = 1 << 3;
    const EF_WASMTIME_PULLEY32: u32 = 1 << 2;

    if let Ok(elf) = ElfFile64::<Endianness>::parse(bytes) {
        match elf.architecture() {
            Architecture::X86_64 => Ok(SupportedTarget::X86_64UnknownNone),
            Architecture::Aarch64 => Ok(SupportedTarget::AArch64UnknownNone),
            Architecture::S390x => {
                Err("Unsupported architecture S390x in AOT compiled file".to_string())
            }
            Architecture::Riscv64 => {
                let e_flags = match elf.flags() {
                    FileFlags::Elf { e_flags, .. } => e_flags,
                    _ => return Err("Unsupported file format in AOT compiled file".to_string()),
                };

                if e_flags & EF_WASMTIME_PULLEY64 != 0 {
                    Ok(SupportedTarget::WasmtimePulley64)
                } else if e_flags & EF_WASMTIME_PULLEY32 != 0 {
                    Err("Unsupported Riscv64 AOT compiled file: pulley32 artifacts are not supported".to_string())
                } else {
                    Err(
                        "Unsupported Riscv64 AOT compiled file, missing expected e_flags in elf header".to_string()
                    )
                }
            }
            other => Err(format!(
                "Unsupported architecture {other:?} in AOT compiled file"
            )),
        }
    } else {
        Err("Failed to parse AOT compiled file as ELF".to_string())
    }
}
