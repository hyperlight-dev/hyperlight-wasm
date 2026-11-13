import 'c.just'

default-target:= "debug"
default-tag:= "latest"
mkdir-arg := if os() == "windows" { "-Force" } else { "-p" }
latest-release:= if os() == "windows" {"$(git tag -l --sort=v:refname | select -last 2 | select -first 1)"} else {`git tag -l --sort=v:refname | tail -n 2 | head -n 1`}
wit-world := if os() == "windows" { "$env:WIT_WORLD=\"" + justfile_directory() + "\\src\\tests\\rust_guests\\component_sample\\wit\\component-world.wasm" + "\";" } else { "WIT_WORLD=" + justfile_directory() + "/src/tests/rust_guests/component_sample/wit/component-world.wasm" }
wit-world-c := if os() == "windows" { "$env:WIT_WORLD=\"" + justfile_directory() + "\\src\\tests\\c_guests\\wasmsamples\\components\\runcomponent-world.wasm" + "\";" } else { "WIT_WORLD=" + justfile_directory() + "/src/tests/c_guests/wasmsamples/components/runcomponent-world.wasm" }
wit-world-name-greeter := if os() == "windows" { "$env:WIT_WORLD_NAME=\"greeter-world\";" } else { "WIT_WORLD_NAME=greeter-world" }

set windows-shell := ["pwsh.exe", "-NoLogo", "-Command"]

make-vendor-tar:
    tar cf ./src/hyperlight_wasm/vendor.tar \
        --owner=0 --group=0 \
        --exclude-vcs-ignores \
        -C ./src wasm_runtime hyperlight_wasm_macro

wasm-tools-version := "1.235.0"
cargo-component-version := "0.21.1"
wit-bindgen-version := "0.58.0"
cargo-hyperlight-version := "0.1.14"

# Installs a crate unless `<command> --version` already reports the pinned version.
[private]
ensure-tool crate version command:
    {{ if os() == "windows" { "if (-not ($(try { " + command + " --version 2>$null } catch {}) -like '*" + version + "*')) { cargo install --locked --force --version " + version + " " + crate + " }" } else { command + " --version 2>/dev/null | grep -qF '" + version + "' || cargo install --locked --force --version " + version + " " + crate } }}

ensure-tools: (ensure-tool "wasm-tools" wasm-tools-version "wasm-tools") (ensure-tool "cargo-component" cargo-component-version "cargo component") (ensure-tool "wit-bindgen-cli" wit-bindgen-version "wit-bindgen") ensure-cargo-hyperlight

ensure-cargo-hyperlight: (ensure-tool "cargo-hyperlight" cargo-hyperlight-version "cargo hyperlight")

build-all target=default-target features="": (build target features) (build-examples target features) 

build target=default-target features="":
    cargo build -p hyperlight-wasm {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features" } else {"--no-default-features -F " + features } }} --verbose --profile={{ if target == "debug" {"dev"} else { target } }}

mkdir-redist target=default-target:
    mkdir {{ mkdir-arg }} build
    mkdir {{ mkdir-arg }} build/{{ target }}

compile-wit:
    wasm-tools component wit ./src/tests/c_guests/wasmsamples/components/runcomponent.wit -w -o ./src/tests/c_guests/wasmsamples/components/runcomponent-world.wasm
    wasm-tools component wit ./src/tests/rust_guests/component_sample/wit/example.wit -w -o ./src/tests/rust_guests/component_sample/wit/component-world.wasm

build-examples target=default-target features="": (build-wasm-examples target features) (build-rust-wasm-examples target features) (build-rust-component-examples target features)

build-rust-wasm-examples target=default-target features="": (mkdir-redist target)
    rustup target add wasm32-unknown-unknown
    cd ./src/tests/rust_guests/rust_wasm_samples && cargo build --target wasm32-unknown-unknown --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo run -p hyperlight-wasm-aot compile {{ if features =~ "gdb" {"--debug"} else {""} }} {{ if features =~ "wasmtime_latest" {"--wasmtime-version latest"} else {""} }} ./src/tests/rust_guests/rust_wasm_samples/target/wasm32-unknown-unknown/{{ target }}/rust_wasm_samples.wasm ./build/{{ target }}/rust_wasm_samples.aot

build-pulley-rust-wasm-examples target=default-target features="": (mkdir-redist target)
    rustup target add wasm32-unknown-unknown
    cd ./src/tests/rust_guests/rust_wasm_samples && cargo build --target wasm32-unknown-unknown --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo run -p hyperlight-wasm-aot compile --pulley {{ if features =~ "gdb" {"--debug"} else {""} }} {{ if features =~ "wasmtime_latest" {"--wasmtime-version latest"} else {""} }} ./src/tests/rust_guests/rust_wasm_samples/target/wasm32-unknown-unknown/{{ target }}/rust_wasm_samples.wasm ./build/{{ target }}/rust_wasm_samples.aot

build-rust-component-examples target=default-target features="": (compile-wit)
    # use cargo component so we don't get all the wasi imports https://github.com/bytecodealliance/cargo-component?tab=readme-ov-file#relationship-with-wasm32-wasip2
    # we also explicitly target wasm32-unknown-unknown since cargo component might try to pull in wasi imports https://github.com/bytecodealliance/cargo-component/issues/290
    rustup target add wasm32-unknown-unknown
    cd ./src/tests/rust_guests/component_sample && cargo component build --target wasm32-unknown-unknown --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo run -p hyperlight-wasm-aot compile {{ if features =~ "gdb" {"--debug"} else {""} }} {{ if features =~ "wasmtime_latest" {"--wasmtime-version latest"} else {""} }} --component ./src/tests/rust_guests/component_sample/target/wasm32-unknown-unknown/{{ target }}/component_sample.wasm ./build/{{ target }}/component_sample.aot
    cd ./src/tests/rust_guests/greeter_sample && cargo component build --target wasm32-unknown-unknown --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo run -p hyperlight-wasm-aot compile {{ if features =~ "gdb" {"--debug"} else {""} }} {{ if features =~ "wasmtime_latest" {"--wasmtime-version latest"} else {""} }} --component ./src/tests/rust_guests/greeter_sample/target/wasm32-unknown-unknown/{{ target }}/greeter_sample.wasm ./build/{{ target }}/greeter_sample.aot

build-pulley-rust-component-examples target=default-target features="": (compile-wit)
    # use cargo component so we don't get all the wasi imports https://github.com/bytecodealliance/cargo-component?tab=readme-ov-file#relationship-with-wasm32-wasip2
    # we also explicitly target wasm32-unknown-unknown since cargo component might try to pull in wasi imports https://github.com/bytecodealliance/cargo-component/issues/290
    rustup target add wasm32-unknown-unknown
    cd ./src/tests/rust_guests/component_sample && cargo component build --target wasm32-unknown-unknown --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo run -p hyperlight-wasm-aot compile --pulley {{ if features =~ "gdb" {"--debug"} else {""} }} {{ if features =~ "wasmtime_latest" {"--wasmtime-version latest"} else {""} }} --component ./src/tests/rust_guests/component_sample/target/wasm32-unknown-unknown/{{ target }}/component_sample.wasm ./build/{{ target }}/component_sample.aot

check target=default-target:
    cargo check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/tests/rust_guests/rust_wasm_samples  && cargo check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/tests/rust_guests/component_sample  && cargo check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/tests/rust_guests/greeter_sample  && cargo check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/hyperlight_wasm_runtime && cargo hyperlight check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/hyperlight_wasm_macro && cargo check --profile={{ if target == "debug" {"dev"} else { target } }}

fmt-check:
    rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/tests/rust_guests/rust_wasm_samples && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/tests/rust_guests/component_sample && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/tests/rust_guests/greeter_sample && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/hyperlight_wasm_runtime && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/hyperlight_wasm_macro && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check

fmt:
    rustup toolchain install nightly -c rustfmt
    cargo +nightly fmt --all
    cd src/tests/rust_guests/rust_wasm_samples &&  cargo +nightly fmt -v --all
    cd src/tests/rust_guests/component_sample &&  cargo +nightly fmt -v --all
    cd src/tests/rust_guests/greeter_sample &&  cargo +nightly fmt -v --all
    cd src/hyperlight_wasm_runtime && cargo +nightly fmt -v --all
    cd src/hyperlight_wasm_macro && cargo +nightly fmt -v --all

# On Windows runners, GH actions extractions/setup-just installs an
# amd64 version of just, instead of a native aarch64 version. This
# seems to be mostly okay, but it means that arch() in the justfile
# will return x86_64 rather than aarch64. So, in aarch64 CI (where
# this problem happens) we use an env var to override the arch.
arch := env("JUST_OVERRIDE_ARCH", arch())

clippy-excluded-features := if arch == "x86_64" { "" } else { "gdb,trace_guest,crashdump" }
clippy-runtime-features := if arch == "x86_64" { ",gdb,trace_guest" } else { "" }

clippy target=default-target: ensure-cargo-hyperlight
    cargo clippy --workspace --exclude hyperlight-wasm-runtime --exclude hyperlight-wasm-macro --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --features "$(cargo metadata --format-version 1 --no-deps | jq -r '[.packages[]|.name as$pkg|.features|keys[]|select(.!="default" and (IN($ARGS.positional[0]//" "|split(",")[])|not))|$pkg+"/"+.]|join(",")' --args {{clippy-excluded-features}})" -- -D warnings
    cd src/tests/rust_guests/rust_wasm_samples &&  cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings
    cd src/tests/rust_guests/component_sample &&  cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings
    cd src/tests/rust_guests/greeter_sample &&  cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings
    # hyperlight_wasm_runtime has mutually exclusive wasmtime features, so we run clippy for each separately with all other features
    cd src/hyperlight_wasm_runtime && cargo hyperlight clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --no-default-features --features wasmtime_latest{{clippy-runtime-features}},pulley -- -D warnings
    cd src/hyperlight_wasm_runtime && cargo hyperlight clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --no-default-features --features wasmtime_lts{{clippy-runtime-features}} -- -D warnings
    cd src/hyperlight_wasm_macro && cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings

# TESTING
# Metrics tests cannot run with other tests they are marked as ignored so that cargo test works
# There may be tests that we really want to ignore so we cant just use --ignored and run then we have to
# specify the test name of the ignored tests that we want to run
# Additionally, we have to run the tests with the function_call_metrics feature enabled separately
# We exclude hyperlight-wasm-aot because it has both wasmtime versions as dependencies and no tests so we don't need to build both versions for testing
test target=default-target features="":
    cargo test --workspace --exclude hyperlight-wasm-aot {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features" } else {"--no-default-features -F " + features } }}  --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo test --workspace --exclude hyperlight-wasm-aot test_metrics {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features -F function_call_metrics" } else {"--no-default-features -F function_call_metrics," + features } }}  --profile={{ if target == "debug" {"dev"} else { target } }} -- --ignored 

set-env-command := if os() == "windows" { "$env:" } else { "export " }
# Cargo launches test and example binaries through the runner in this variable.
no-run-runner-var := "CARGO_TARGET_" + uppercase(arch) + (if os() == "windows" { "_PC_WINDOWS_MSVC" } else if os() == "macos" { "_APPLE_DARWIN" } else { "_UNKNOWN_LINUX_GNU" }) + "_RUNNER"
# Cargo runs `<runner> <binary> <args>`. Both forms exit 0 and ignore the rest.
no-run-runner := if os() == "windows" { "cmd /c rem" } else { "true" }

# Builds what a CI build job compiles, to fill the CI cache. Tests and examples build but do not run.
prime-caches target=default-target features="":
    just build {{ target }} {{ features }}
    {{ set-env-command }}{{ no-run-runner-var }}='{{ no-run-runner }}'; just test {{ target }} {{ features }} && just examples-ci-prebuilt {{ target }} {{ features }}

examples-modules target=default-target features="": (build-wasm-examples target features) (build-rust-wasm-examples target features) (run-examples-modules target features)

run-examples-modules target=default-target features="":
    cargo run {{ if features =="" {''} else {"--no-default-features -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example helloworld
    cargo run {{ if features =="" {''} else {"--no-default-features -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example hostfuncs
    cargo run {{ if features =="" {''} else {"--no-default-features -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example rust_wasm_examples
    cargo run {{ if features =="" {''} else {"--no-default-features -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example interruption
    cargo run {{ if features =="" {''} else {"--no-default-features -F function_call_metrics," + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example metrics
    cargo run {{ if features =="" {"--no-default-features --features kvm,mshv3"} else {"--no-default-features -F function_call_metrics," + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example metrics

examples-ci target=default-target features="": (examples-modules target features) (examples-components target features) (examples-pulley target features)

# Runs the examples-ci examples on the modules build-ci-examples builds.
examples-ci-prebuilt target=default-target features="": (run-examples-modules target features) (run-examples-components target features) (use-pulley-examples target) (run-examples-pulley target features)

# Builds every module the CI build jobs load, for the given profiles. C modules build
# in release only. Pulley builds go to build/<profile>/pulley.
build-ci-examples features="" profiles="debug release": (build-wasm-examples "release" features)
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ " {{ profiles }} " == *" debug "* ]]; then
        mkdir -p build/debug
        cp build/release/*.aot build/debug/
    fi
    for target in {{ profiles }}; do
        just build-pulley-rust-wasm-examples $target {{ features }}
        just build-pulley-rust-component-examples $target {{ features }}
        mkdir -p build/$target/pulley
        mv build/$target/rust_wasm_samples.aot build/$target/component_sample.aot build/$target/pulley/
        just build-rust-wasm-examples $target {{ features }}
        just build-rust-component-examples $target {{ features }}
    done

[private]
use-pulley-examples target=default-target:
    {{ if os() == "windows" { "Copy-Item -Force" } else { "cp" } }} build/{{ target }}/pulley/*.aot build/{{ target }}/

examples-components target=default-target features="": (build-rust-component-examples target features) (run-examples-components target features)

run-examples-components target=default-target features="":
    {{ wit-world }} cargo run {{ if features =="" {''} else {"--no-default-features -F kvm -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example component_example
    {{ wit-world }} cargo test -p hyperlight-wasm {{ if features =="" {''} else {"--no-default-features -F kvm -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --lib snapshot::tests::component
    {{ wit-world-c }} cargo run {{ if features =="" {''} else {"--no-default-features -F kvm -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example c-component
    {{ wit-world }} {{ wit-world-name-greeter }} cargo run {{ if features =="" {''} else {"--no-default-features -F kvm -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example component_greeter_example
    {{ wit-world }} cargo test -p hyperlight-wasm-capi {{ if features =="" {''} else {"--no-default-features -F kvm -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} -- --include-ignored

# Builds the native libraries and a guest runtime for the component_sample example world, for the .NET tests
dotnet-test-assets target=default-target features="": ensure-cargo-hyperlight
    cargo build -p hyperlight-wasm-capi -p hyperlight-wasm-csharp-bindgen {{ if features =="" {''} else {"--no-default-features -F kvm -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }}
    {{ wit-world }} cargo hyperlight build {{ if features =~ "wasmtime_latest" {"--no-default-features -F wasmtime_latest"} else {""} }} --profile={{ if target == "debug" {"dev"} else { target } }} --manifest-path src/hyperlight_wasm_runtime/Cargo.toml --target-dir target/component-sample-runtime --locked

# Runs the .NET tests. Needs the .NET 8 SDK or later.
dotnet-test target=default-target features="": (build-rust-component-examples target features) (dotnet-test-prebuilt target features)

# Runs the .NET tests on the component_sample component that build-ci-examples builds.
dotnet-test-prebuilt target=default-target features="": (dotnet-test-assets target features)
    dotnet test dotnet/tests/Hyperlight.Wasm.Tests -c {{ if target == "debug" {"Debug"} else {"Release"} }}

# Packs the .NET packages for this machine's runtime identifier into dotnet/artifacts
dotnet-pack target="release":
    cargo build -p hyperlight-wasm-capi -p hyperlight-wasm-csharp-bindgen --profile={{ if target == "debug" {"dev"} else { target } }}
    dotnet pack dotnet/src/Hyperlight.Wasm -c {{ if target == "debug" {"Debug"} else {"Release"} }} -o dotnet/artifacts
    dotnet pack dotnet/src/Hyperlight.Wasm.Build -c {{ if target == "debug" {"Debug"} else {"Release"} }} -o dotnet/artifacts

# Test a component and a module compiled with pulley
examples-pulley target=default-target features="": (build-pulley-rust-component-examples target features) (build-pulley-rust-wasm-examples target features) (run-examples-pulley target features)

run-examples-pulley target=default-target features="":
    {{ wit-world }} cargo run {{ if features =="" {'-F pulley'} else {"--no-default-features -F kvm,pulley -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example component_example
    cargo run {{ if features =="" {'-F pulley'} else {"--no-default-features -F pulley -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example rust_wasm_examples

# warning, compares to and then OVERWRITES the given baseline
bench-ci baseline target="release" features="":
    cd src/hyperlight_wasm && cargo bench --profile={{ if target == "debug" {"dev"} else { target } }} {{ if features =="" {''} else { "--features " + features } }} --bench benchmarks -- --verbose --save-baseline {{baseline}}
    cd src/hyperlight_wasm; {{wit-world-c}} cargo bench --profile={{ if target == "debug" {"dev"} else { target } }} {{ if features =="" {''} else { "--features " + features } }} --bench benchmarks_components -- --verbose --save-baseline {{baseline}}-components
bench target="release" features="": (bench-wasm target features) (bench-components target features)
bench-wasm target="release" features="":
    cd src/hyperlight_wasm &&  cargo bench --profile={{ if target == "debug" {"dev"} else { target } }} {{ if features =="" {''} else { "--features " + features } }} --bench benchmarks -- --verbose 
bench-components target="release" features="":
    cd src/hyperlight_wasm; {{wit-world-c}} cargo bench --profile={{ if target == "debug" {"dev"} else { target } }} {{ if features =="" {''} else { "--features " + features } }} --bench benchmarks_components -- --verbose
bench-download os hypervisor cpu tag="":
    gh release download {{ tag }} -D ./src/hyperlight_wasm/target/ -p benchmarks_{{ os }}_{{ hypervisor }}_{{ cpu }}.tar.gz
    mkdir {{ mkdir-arg }} ./src/hyperlight_wasm/target/criterion
    tar -zxvf ./src/hyperlight_wasm/target/benchmarks_{{ os }}_{{ hypervisor }}_{{ cpu }}.tar.gz -C ./src/hyperlight_wasm/target/criterion/ --strip-components=1
