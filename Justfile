default-target:= "debug"
default-tag:= "latest"
build-wasm-examples-command := if os() == "windows" { "./src/hyperlight_wasm/scripts/build-wasm-examples.bat" } else { "./src/hyperlight_wasm/scripts/build-wasm-examples.sh" }
mkdir-arg := if os() == "windows" { "-Force" } else { "-p" }
latest-release:= if os() == "windows" {"$(git tag -l --sort=v:refname | select -last 2 | select -first 1)"} else {`git tag -l --sort=v:refname | tail -n 2 | head -n 1`}
wit-world := if os() == "windows" { "$env:WIT_WORLD=\"" + justfile_directory() + "\\src\\component_sample\\wit\\component-world.wasm" + "\";" } else { "WIT_WORLD=" + justfile_directory() + "/src/component_sample/wit/component-world.wasm" }
wit-world-c := if os() == "windows" { "$env:WIT_WORLD=\"" + justfile_directory() + "\\src\\wasmsamples\\components\\runcomponent-world.wasm" + "\";" } else { "WIT_WORLD=" + justfile_directory() + "/src/wasmsamples/components/runcomponent-world.wasm" }

set windows-shell := ["pwsh.exe", "-NoLogo", "-Command"]

make-vendor-tar:
    tar cf ./src/hyperlight_wasm/vendor.tar \
        --owner=0 --group=0 \
        --exclude-vcs-ignores \
        -C ./src wasm_runtime hyperlight_wasm_macro

ensure-tools:
    cargo install wasm-tools --locked --version 1.235.0
    cargo install cargo-component --locked --version 0.21.1
    cargo install wit-bindgen-cli --locked --version 0.43.0

build-all target=default-target features="": (build target features) (build-wasm-examples target features) (build-rust-wasm-examples target features) (build-rust-component-examples target) (build-wasm-runtime target features)

build target=default-target features="": (fmt-check)
    cargo build {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features" } else {"--no-default-features -F " + features } }} --verbose --profile={{ if target == "debug" {"dev"} else { target } }}

mkdir-redist target=default-target:
    mkdir {{ mkdir-arg }} x64
    mkdir {{ mkdir-arg }} x64/{{ target }}

compile-wit:
    wasm-tools component wit ./src/wasmsamples/components/runcomponent.wit -w -o ./src/wasmsamples/components/runcomponent-world.wasm
    wasm-tools component wit ./src/component_sample/wit/example.wit -w -o ./src/component_sample/wit/component-world.wasm

build-wasm-runtime target=default-target features="":
    cd ./src/wasm_runtime && cargo build --verbose {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features" } else {"--features " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} && rm -R target

build-wasm-examples target=default-target features="": (compile-wit)
    {{ build-wasm-examples-command }} {{target}} {{features}}

build-rust-wasm-examples target=default-target features="": (mkdir-redist target)
    rustup target add wasm32-unknown-unknown
    cd ./src/rust_wasm_samples && cargo build --target wasm32-unknown-unknown --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo run {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features" } else {"--features " + features } }} -p hyperlight-wasm-aot compile {{ if features =~ "gdb" {"--debug"} else {""} }} ./src/rust_wasm_samples/target/wasm32-unknown-unknown/{{ target }}/rust_wasm_samples.wasm ./x64/{{ target }}/rust_wasm_samples.aot

build-rust-component-examples target=default-target features="": (compile-wit)
    # use cargo component so we don't get all the wasi imports https://github.com/bytecodealliance/cargo-component?tab=readme-ov-file#relationship-with-wasm32-wasip2
    # we also explicitly target wasm32-unknown-unknown since cargo component might try to pull in wasi imports https://github.com/bytecodealliance/cargo-component/issues/290
    rustup target add wasm32-unknown-unknown
    cd ./src/component_sample && cargo component build --target wasm32-unknown-unknown --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo run {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features" } else {"--features " + features } }} -p hyperlight-wasm-aot compile {{ if features =~ "gdb" {"--debug"} else {""} }} --component ./src/component_sample/target/wasm32-unknown-unknown/{{ target }}/component_sample.wasm ./x64/{{ target }}/component_sample.aot

check target=default-target:
    cargo check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/rust_wasm_samples  && cargo check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/component_sample  && cargo check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/wasm_runtime && cargo check --profile={{ if target == "debug" {"dev"} else { target } }}
    cd src/hyperlight_wasm_macro && cargo check --profile={{ if target == "debug" {"dev"} else { target } }}

fmt-check:
    rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/rust_wasm_samples && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/component_sample && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/wasm_runtime && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check
    cd src/hyperlight_wasm_macro && rustup toolchain install nightly -c rustfmt && cargo +nightly fmt -v --all -- --check

fmt:
    rustup toolchain install nightly -c rustfmt
    cargo +nightly fmt --all
    cd src/rust_wasm_samples &&  cargo +nightly fmt -v --all
    cd src/component_sample &&  cargo +nightly fmt -v --all
    cd src/wasm_runtime && cargo +nightly fmt -v --all
    cd src/hyperlight_wasm_macro && cargo +nightly fmt -v --all

export CC_x86_64_unknown_none:= if os() == "windows" { justfile_directory() / "src/wasm_runtime/guest-toolchain/clang" } else { "" }
export AR_x86_64_unknown_none:= if os() == "windows" { "llvm-ar" } else { "" }

clippy target=default-target: (check target)
    cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings
    cd src/rust_wasm_samples &&  cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings
    cd src/component_sample &&  cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings
    cd src/wasm_runtime && cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings
    cd src/hyperlight_wasm_macro && cargo clippy --profile={{ if target == "debug" {"dev"} else { target } }} --all-targets --all-features -- -D warnings

# TESTING
# Metrics tests cannot run with other tests they are marked as ignored so that cargo test works
# There may be tests that we really want to ignore so we cant just use --ignored and run then we have to
# specify the test name of the ignored tests that we want to run
# Additionally, we have to run the tests with the function_call_metrics feature enabled separately
test target=default-target features="":
    cargo test {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features" } else {"--no-default-features -F " + features } }}  --profile={{ if target == "debug" {"dev"} else { target } }}
    cargo test test_metrics {{ if features =="" {''} else if features=="no-default-features" {"--no-default-features" } else {"--no-default-features -F " + features } }}  --profile={{ if target == "debug" {"dev"} else { target } }} -- --ignored 

examples-ci target=default-target features="": (build-rust-wasm-examples target)
    cargo run {{ if features =="" {''} else {"--no-default-features -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example helloworld
    cargo run {{ if features =="" {''} else {"--no-default-features -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example hostfuncs
    cargo run {{ if features =="" {''} else {"--no-default-features -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example rust_wasm_examples
    cargo run {{ if features =="" {''} else {"--no-default-features -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example interruption
    cargo run {{ if features =="" {''} else {"--no-default-features -F function_call_metrics," + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example metrics
    cargo run {{ if features =="" {"--no-default-features --features kvm,mshv3"} else {"--no-default-features -F function_call_metrics," + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example metrics 

examples-components target=default-target features="": (build-rust-component-examples target) 
    {{ wit-world }} cargo run {{ if features =="" {''} else {"--no-default-features -F kvm -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example component_example
    {{ wit-world-c }} cargo run {{ if features =="" {''} else {"--no-default-features -F kvm -F " + features } }} --profile={{ if target == "debug" {"dev"} else { target } }} --example c-component

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
