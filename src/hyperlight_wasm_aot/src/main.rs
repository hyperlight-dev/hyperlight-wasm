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
use std::path::Path;

use cargo_metadata::{MetadataCommand, Package};
use cargo_util_schemas::manifest::PackageName;
use clap::{Parser, Subcommand};
use object::read::elf::ElfFile64;
use object::{Architecture, Endianness, FileFlags, Object};
use wasmtime::{Config, Engine, Module, OptLevel, Precompiled};

#[derive(Debug)]
enum SupportedTarget {
    X86_64UnknownNone,
    WasmtimePulley64,
}

impl Display for SupportedTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupportedTarget::X86_64UnknownNone => write!(f, "x86_64-unknown-none"),
            SupportedTarget::WasmtimePulley64 => write!(f, "pulley64"),
        }
    }
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
    },

    /// Check which Wasmtime version was used to precompile a module
    CheckWasmtimeVersion {
        /// The precompiled file to check
        file: String,

        /// Specifies if the module has been compiled with debug support
        #[arg(long)]
        debug: bool,
    },
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
        } => {
            let outfile = match output {
                Some(s) => s,
                None => {
                    let mut path = Path::new(&input).to_path_buf();
                    path.set_extension("aot");
                    path.to_str().unwrap().to_string()
                }
            };
            let target = if pulley {
                SupportedTarget::WasmtimePulley64
            } else {
                SupportedTarget::X86_64UnknownNone
            };
            if debug {
                println!(
                    "Aot Compiling {} to [{}]: {} with debug info and optimizations off",
                    input, target, outfile
                );
            } else {
                println!("Aot Compiling {} to [{}]: {}", input, target, outfile);
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
        Commands::CheckWasmtimeVersion { file, debug } => {
            // get the wasmtime version used by hyperlight-wasm-aot
            let metadata = MetadataCommand::new().exec().unwrap();
            let package_name = PackageName::new("wasmtime".to_string()).unwrap();
            let wasmtime_package: Option<&Package> =
                metadata.packages.iter().find(|p| p.name == package_name);
            let version_number = match wasmtime_package {
                Some(pkg) => pkg.version.clone(),
                None => panic!("wasmtime dependency not found"),
            };
            if debug {
                println!(
                    "Checking Wasmtime version used to compile debug info enabled file: {}",
                    file
                );
            } else {
                println!("Checking Wasmtime version used to compile file: {}", file);
            }
            // load the file into wasmtime, check that it is aot compiled and extract the version of wasmtime used to compile it from its metadata
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
            Architecture::Aarch64 => {
                Err("Unsupported architecture Aarch64 in AOT compiled file".to_string())
            }
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
