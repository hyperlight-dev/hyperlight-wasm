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

use clap::{Parser, Subcommand, ValueEnum};
use object::read::elf::ElfFile64;
use object::{Architecture, Endianness, FileFlags, Object};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum WasmtimeVersion {
    Lts,
    Latest,
}
impl Display for WasmtimeVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmtimeVersion::Lts => write!(f, "LTS"),
            WasmtimeVersion::Latest => write!(f, "latest"),
        }
    }
}
impl WasmtimeVersion {
    fn dependency_version(self) -> &'static str {
        match self {
            WasmtimeVersion::Lts => std::env!("WASMTIME_LTS_VERSION"),
            WasmtimeVersion::Latest => std::env!("WASMTIME_LATEST_VERSION"),
        }
    }
}
enum WasmtimeVersioned<Lts, Latest> {
    Lts(Lts),
    Latest(Latest),
}
macro_rules! wvt {
    ($t:ident) => {
        type $t = WasmtimeVersioned<wasmtime_lts::$t, wasmtime::$t>;
    };
}
wvt!(Config);
impl Config {
    fn new(wv: WasmtimeVersion) -> Config {
        match wv {
            WasmtimeVersion::Lts => WasmtimeVersioned::Lts(wasmtime_lts::Config::new()),
            WasmtimeVersion::Latest => WasmtimeVersioned::Latest(wasmtime::Config::new()),
        }
    }
}

macro_rules! wv {
    (@impl [] [$($acc:tt)*] $v:ident, $($_:tt)*) => {
        match &mut $v {
            $($acc)*
        }
    };
    (@impl [nonexhaustive] [$($acc:tt)*] $v:ident, $($x:tt)*) => {
        wv!(@impl [] [$($acc)* _ => {}] $v, $($x)*)
    };
    (@impl [$variant:ident $module:ident $($scope:tt)*] [$($acc:tt)*] $v:ident, $($x:tt)*) => {
        wv!(@impl [$($scope)*] [$($acc)*
          WasmtimeVersioned::$variant($v) => {
              #[allow(unused)]
              use $module as w;
              $($x)*
          },
        ] $v, $($x)*)
    };
    (@scoped [$($scope:tt)*], $v:ident . $($x:tt)*) => {
        wv!(@impl [$($scope)*] [] $v, $v.$($x)*;)
    };
    (@scoped [$($scope:tt)*], $v:ident, $($x:tt)*) => {
        wv!(@impl [$($scope)*] [] $v, $($x)*)
    };
    (latest, $($x:tt)*) => {
        wv!(@scoped [Latest wasmtime nonexhaustive], $($x)*)
    };
    (lts, $($x:tt)*) => {
        wv!(@scoped [Lts wasmtime_lts nonexhaustive], $($x)*)b
    };
    ($($x:tt)*) => {
        wv!(@scoped [Lts wasmtime_lts Latest wasmtime], $($x)*)
    };
}
macro_rules! wvwv {
    ($v:ident, $($x:tt)*) => {
        match $v {
            WasmtimeVersioned::Lts($v) => {
                #[allow(unused)]
                use wasmtime_lts as w;
                WasmtimeVersioned::Lts($($x)*)
            }
            WasmtimeVersioned::Latest($v) => {
                #[allow(unused)]
                use wasmtime as w;
                WasmtimeVersioned::Latest($($x)*)
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum SupportedTarget {
    X86_64UnknownHyperlight,
    AArch64UnknownHyperlight,
    WasmtimePulley64,
}
impl SupportedTarget {
    fn wasmtime_target(self) -> String {
        match self {
            SupportedTarget::X86_64UnknownHyperlight => "x86_64-unknown-hyperlight".to_string(),
            SupportedTarget::AArch64UnknownHyperlight => "aarch64-unknown-hyperlight".to_string(),
            SupportedTarget::WasmtimePulley64 => "pulley64".to_string(),
        }
    }
    fn apply_wasmtime_config(self, mut config: &mut Config) {
        wv!(config.target(&self.wasmtime_target()).unwrap());
        if let SupportedTarget::X86_64UnknownHyperlight = self {
            // Enable x86_float_abi_ok only for the latest Wasmtime version.
            // Safety:
            // - If using the new upstream x86_64-unknown-hyperlight
            //   targets, they are hard float already
            // - Otherwise, we use cargo-hyperlight to build the
            //   guest, which sets the Rust target to be compiled with
            //   the hard-float ABI manually via `-Zbuild-std` and a
            //   custom target JSON configuration See
            //   https://github.com/bytecodealliance/wasmtime/pull/11553
            unsafe {
                wv!(latest, config.x86_float_abi_ok(true));
            }
        }
    }
}
impl Display for SupportedTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.wasmtime_target())
    }
}

#[derive(Clone, Copy, Parser)]
struct TargetSelection {
    /// Pre-compile for the pulley64 target
    #[arg(long)]
    pulley: bool,
}

impl From<TargetSelection> for SupportedTarget {
    fn from(ts: TargetSelection) -> SupportedTarget {
        if ts.pulley {
            Self::WasmtimePulley64
        } else {
            #[cfg(target_arch = "x86_64")]
            let target = Self::X86_64UnknownHyperlight;
            #[cfg(target_arch = "aarch64")]
            let target = Self::AArch64UnknownHyperlight;
            target
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

        #[command(flatten)]
        target_selection: TargetSelection,

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
            target_selection,
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

            let target = target_selection.into();

            if debug {
                println!(
                    "Aot Compiling {} to [{}]: {} with debug info and optimizations off ({} wasmtime)",
                    input, target, outfile, wasmtime_version
                );
            } else {
                println!(
                    "Aot Compiling {} to [{}]: {} ({} wasmtime)",
                    input, target, outfile, wasmtime_version
                );
            }

            let config = get_config(wasmtime_version, debug, minimal, &target);
            let mut engine = wvwv!(config, w::Engine::new(&config).unwrap());
            let bytes = std::fs::read(&input).unwrap();
            let serialized = if component {
                wv!(engine, engine.precompile_component(&bytes).unwrap())
            } else {
                wv!(engine, engine.precompile_module(&bytes).unwrap())
            };
            std::fs::write(outfile, serialized).unwrap();
        }
        Commands::CheckWasmtimeVersion {
            file,
            debug,
            wasmtime_version,
        } => {
            if debug {
                println!(
                    "Checking Wasmtime version used to compile with debug info enabled file: {} ({} wasmtime)",
                    file, wasmtime_version
                );
            } else {
                println!(
                    "Checking Wasmtime version used to compile file: {} ({} wasmtime)",
                    file, wasmtime_version
                );
            }

            // get the wasmtime version used by hyperlight-wasm-aot
            let version_number = wasmtime_version.dependency_version();

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
            let config = get_config(wasmtime_version, debug, false, &target);
            let engine = wvwv!(config, w::Engine::new(&config).unwrap());
            wvwv!(
                engine,
                match w::Engine::detect_precompiled(&bytes) {
                    Some(pre_compiled) => {
                        match pre_compiled {
                            w::Precompiled::Module => {
                                println!("The file is a valid AOT compiled Wasmtime module");
                                // It doesnt seem like the functions or data needed to extract the version of wasmtime used to compile the module are exposed in the wasmtime crate
                                // so we will try and load it and then catch the error and parse the version from the error message :-(
                                match unsafe { w::Module::deserialize(&engine, bytes) } {
                                    Ok(_) => println!(
                                        "File {} was AOT compiled to '{}' with wasmtime version compatible with: {}",
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
                            w::Precompiled::Component => {
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
            );
        }
    }
}

/// Returns a new `Config` for the Wasmtime engine with additional settings for AOT compilation.
fn get_config(wv: WasmtimeVersion, debug: bool, minimal: bool, target: &SupportedTarget) -> Config {
    let mut config = Config::new(wv);

    target.apply_wasmtime_config(&mut config);

    // Enable the default features for the Wasmtime engine.
    if debug {
        wv!(config.debug_info(true));
        wv!(config.cranelift_opt_level(w::OptLevel::None));
    }

    if minimal {
        wv!(config.generate_address_map(false));
        wv!(config.native_unwind_info(false));
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
            Architecture::X86_64 => Ok(SupportedTarget::X86_64UnknownHyperlight),
            Architecture::Aarch64 => Ok(SupportedTarget::AArch64UnknownHyperlight),
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
