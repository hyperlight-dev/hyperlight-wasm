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
use clap::{Arg, Command};
use wasmtime::{Config, Engine, Module, OptLevel, Precompiled};

fn main() {
    let hyperlight_wasm_aot_version = env!("CARGO_PKG_VERSION");
    let matches = Command::new("hyperlight-wasm-aot")
        .version(hyperlight_wasm_aot_version)
        .about("AOT compilation for hyperlight-wasm")
        .subcommand(
            Command::new("compile")
                .about("Compile a wasm file to an AOT file")
                .arg(
                    Arg::new("input")
                        .help("The input wasm file")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("output")
                        .help("The output AOT file")
                        .required(false)
                        .index(2),
                )
                .arg(
                    Arg::new("component")
                        .help("Compile a component rather than a module")
                        .required(false)
                        .long("component")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("debug")
                        .help("Precompile with debug and disable optimizations")
                        .required(false)
                        .long("debug")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("check-wasmtime-version")
                .about("Check the Wasmtime version use to compile a AOT file")
                .arg(
                    Arg::new("file")
                        .help("The aot compiled file to check")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("debug")
                        .help("Specifies if the module has been compiled with debug support")
                        .required(false)
                        .long("debug")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .get_matches();

    match matches.subcommand_name() {
        Some("compile") => {
            let args = matches.subcommand_matches("compile").unwrap();
            let infile = args.get_one::<String>("input").unwrap();
            let outfile = match args.get_one::<String>("output") {
                Some(s) => s.clone(),
                None => {
                    let mut path = Path::new(infile).to_path_buf();
                    path.set_extension("aot");
                    path.to_str().unwrap().to_string().clone()
                }
            };
            let debug = args.get_flag("debug");
            if debug {
                println!(
                    "Aot Compiling {} to {} with debug info and optimizations off",
                    infile, outfile
                );
            } else {
                println!("Aot Compiling {} to {}", infile, outfile);
            }
            let config = get_config(debug);
            let engine = Engine::new(&config).unwrap();
            let bytes = std::fs::read(infile).unwrap();
            let serialized = if args.get_flag("component") {
                engine.precompile_component(&bytes).unwrap()
            } else {
                engine.precompile_module(&bytes).unwrap()
            };
            std::fs::write(outfile, serialized).unwrap();
        }
        Some("check-wasmtime-version") => {
            // get the wasmtime version used by hyperlight-wasm-aot
            let metadata = MetadataCommand::new().exec().unwrap();
            let package_name = PackageName::new("wasmtime".to_string()).unwrap();
            let wasmtime_package: Option<&Package> =
                metadata.packages.iter().find(|p| p.name == package_name);
            let version_number = match wasmtime_package {
                Some(pkg) => pkg.version.clone(),
                None => panic!("wasmtime dependency not found"),
            };
            let args = matches
                .subcommand_matches("check-wasmtime-version")
                .unwrap();
            let debug = args.get_flag("debug");
            let file = args.get_one::<String>("file").unwrap();
            if debug {
                println!(
                    "Checking Wasmtime version used to compile debug info enabled file: {}",
                    file
                );
            } else {
                println!("Checking Wasmtime version used to compile file: {}", file);
            }
            // load the file into wasmtime, check that it is aot compiled and extract the version of wasmtime used to compile it from its metadata
            let bytes = std::fs::read(file).unwrap();
            let config = get_config(debug);
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
        _ => {
            println!("No subcommand specified");
        }
    }
}

/// Returns a new `Config` for the Wasmtime engine with additional settings for AOT compilation.
fn get_config(debug: bool) -> Config {
    let mut config = Config::new();
    config.target("x86_64-unknown-none").unwrap();

    // Enable the default features for the Wasmtime engine.
    if debug {
        config.debug_info(true);
        config.cranelift_opt_level(OptLevel::None);
    }

    config
}
