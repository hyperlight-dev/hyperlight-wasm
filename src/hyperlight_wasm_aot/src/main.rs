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

use std::path::Path;

use cargo_metadata::{MetadataCommand, Package};
use cargo_util_schemas::manifest::PackageName;
use clap::{Parser, Subcommand};
use wasmtime::{Config, Engine, Module, OptLevel, Precompiled};

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
        } => {
            let outfile = match output {
                Some(s) => s,
                None => {
                    let mut path = Path::new(&input).to_path_buf();
                    path.set_extension("aot");
                    path.to_str().unwrap().to_string()
                }
            };
            if debug {
                println!(
                    "Aot Compiling {} to {} with debug info and optimizations off",
                    input, outfile
                );
            } else {
                println!("Aot Compiling {} to {}", input, outfile);
            }
            let config = get_config(debug, minimal);
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
            let config = get_config(debug, false);
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
                                    "File {} was AOT compiled with wasmtime version: {}",
                                    file, version_number
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
                                        "File {} was AOT compiled with wasmtime version: {}",
                                        file, version
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
fn get_config(debug: bool, minimal: bool) -> Config {
    let mut config = Config::new();
    config.target("x86_64-unknown-none").unwrap();

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
