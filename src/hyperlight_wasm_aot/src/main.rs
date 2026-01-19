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

use clap::{Arg, Command};

fn precompile_bytes(bytes: &[u8], debug: bool, use_lts: bool, is_component: bool) -> Vec<u8> {
    if use_lts {
        let mut config = wasmtime_lts::Config::new();
        config.target("x86_64-unknown-none").unwrap();
        if debug {
            config.debug_info(true);
            config.cranelift_opt_level(wasmtime_lts::OptLevel::None);
        }
        let engine = wasmtime_lts::Engine::new(&config).unwrap();
        if is_component {
            engine.precompile_component(bytes).unwrap()
        } else {
            engine.precompile_module(bytes).unwrap()
        }
    } else {
        let mut config = wasmtime::Config::new();
        config.target("x86_64-unknown-none").unwrap();
        unsafe { config.x86_float_abi_ok(true) };
        if debug {
            config.debug_info(true);
            config.cranelift_opt_level(wasmtime::OptLevel::None);
        }
        let engine = wasmtime::Engine::new(&config).unwrap();
        if is_component {
            engine.precompile_component(bytes).unwrap()
        } else {
            engine.precompile_module(bytes).unwrap()
        }
    }
}

fn detect_and_deserialize(
    bytes: &[u8],
    debug: bool,
    use_lts: bool,
    file: &str,
    version_label: &str,
) {
    if use_lts {
        let mut config = wasmtime_lts::Config::new();
        config.target("x86_64-unknown-none").unwrap();
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
                        "File {} was AOT compiled with a compatible wasmtime version ({})",
                        file, version_label
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
    } else {
        let mut config = wasmtime::Config::new();
        config.target("x86_64-unknown-none").unwrap();
        // Enable x86_float_abi_ok only for the latest Wasmtime version.
        // Safety:
        // We are using hyperlight cargo to build the guest which
        // sets the Rust target to be compiled with the hard-float ABI manually via
        // `-Zbuild-std` and a custom target JSON configuration
        // See https://github.com/bytecodealliance/wasmtime/pull/11553
        unsafe { config.x86_float_abi_ok(true) };
        if debug {
            config.debug_info(true);
            config.cranelift_opt_level(wasmtime::OptLevel::None);
        }
        let engine = wasmtime::Engine::new(&config).unwrap();
        match wasmtime::Engine::detect_precompiled(bytes) {
            Some(wasmtime::Precompiled::Module) => {
                println!("The file is a valid AOT compiled Wasmtime module");
                match unsafe { wasmtime::Module::deserialize(&engine, bytes) } {
                    Ok(_) => println!(
                        "File {} was AOT compiled with a compatible wasmtime version ({})",
                        file, version_label
                    ),
                    Err(e) => eprintln!("{}", e),
                }
            }
            Some(wasmtime::Precompiled::Component) => {
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
}

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
                )
                .arg(
                    Arg::new("lts")
                        .help("Use LTS wasmtime version instead of latest")
                        .required(false)
                        .long("lts")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("check-wasmtime-version")
                .about("Check the Wasmtime version used to compile an AOT file")
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
                )
                .arg(
                    Arg::new("lts")
                        .help("Use LTS wasmtime version instead of latest")
                        .required(false)
                        .long("lts")
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
            let use_lts = args.get_flag("lts");
            let is_component = args.get_flag("component");
            let version_label = if use_lts { "LTS" } else { "latest" };

            if debug {
                println!(
                    "Aot Compiling {} to {} with debug info and optimizations ({} wasmtime)",
                    infile, outfile, version_label
                );
            } else {
                println!(
                    "Aot Compiling {} to {} ({} wasmtime)",
                    infile, outfile, version_label
                );
            }

            let bytes = std::fs::read(infile).unwrap();
            let serialized = precompile_bytes(&bytes, debug, use_lts, is_component);
            std::fs::write(outfile, serialized).unwrap();
        }
        Some("check-wasmtime-version") => {
            let args = matches
                .subcommand_matches("check-wasmtime-version")
                .unwrap();
            let debug = args.get_flag("debug");
            let use_lts = args.get_flag("lts");
            let file = args.get_one::<String>("file").unwrap();
            let version_label = if use_lts { "LTS" } else { "latest" };

            if debug {
                println!(
                    "Checking Wasmtime version used to compile with debug info enabled file: {} ({} wasmtime)",
                    file, version_label
                );
            } else {
                println!(
                    "Checking Wasmtime version used to compile file: {} ({} wasmtime)",
                    file, version_label
                );
            }

            let bytes = std::fs::read(file).unwrap();
            detect_and_deserialize(&bytes, debug, use_lts, file, version_label);
        }
        _ => {
            println!("No subcommand specified");
        }
    }
}
