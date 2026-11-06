wasi-sdk-version-major := "25"
wasi-sdk-version := wasi-sdk-version-major + ".0"
# wasi-sdk 25 has no Windows ARM64 build. The x86_64 build runs under emulation.
wasi-sdk-name := "wasi-sdk-" + wasi-sdk-version + "-" + (if os() == "windows" { "x86_64-windows" } else { (if arch == "aarch64" { "arm64" } else { "x86_64" }) + "-" + (if os() == "macos" { "macos" } else { "linux" }) })
# The devcontainer installs wasi-sdk in /opt/wasi-sdk. Elsewhere ensure-wasi-sdk downloads it into target.
wasi-sdk := env("WASI_SDK_PATH", if path_exists("/opt/wasi-sdk") == "true" { "/opt/wasi-sdk" } else { justfile_directory() / "target" / wasi-sdk-name })
wasm-samples := "src/tests/c_guests/wasmsamples"

# clang runs any wasm-opt on PATH when optimizing. A PATH of only wasi-sdk's bin keeps the output independent of it.
wasi-clang := set-env-command + "PATH='" + (wasi-sdk / "bin") + "'; " + (if os() == "windows" { "& " } else { "" }) + "'" + (wasi-sdk / "bin" / "clang") + "'"
# Single quotes keep pwsh from reading the commas in -Wl flags as array separators.
c-wasm-flags := "-ffunction-sections -mexec-model=reactor -z stack-size=4096 '-Wl,--initial-memory=65536' '-Wl,--export=__data_end' '-Wl,--export=__heap_base,--export=malloc,--export=free,--export=__wasm_call_ctors' '-Wl,--no-entry' '-Wl,--allow-undefined' '-Wl,--gc-sections'"
c-wasm-flags-debug := "-O0"
c-wasm-flags-release := "-O3 '-Wl,--strip-all'"

build-wasm-examples target=default-target features="": (compile-wit) ensure-wasi-sdk (mkdir-redist target) (build-c-module "HelloWorld" target features) (build-c-module "HostFunction" target features) (build-c-module "RunWasm" target features) (build-c-component "runcomponent" target features)

# Extracts into a staging directory and moves the SDK into place, so an interrupted extraction is retried.
ensure-wasi-sdk:
    {{ if path_exists(wasi-sdk) == "true" { "" } else { "mkdir " + mkdir-arg + " target/wasi-sdk-staging && curl -fsSL --retry 3 -o target/" + wasi-sdk-name + ".tar.gz https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-" + wasi-sdk-version-major + "/" + wasi-sdk-name + ".tar.gz && tar -xzf target/" + wasi-sdk-name + ".tar.gz -C target/wasi-sdk-staging && mv target/wasi-sdk-staging/" + wasi-sdk-name + " target/" + wasi-sdk-name } }}

[private]
build-c-module name target features:
    {{ wasi-clang }} {{ if target == "debug" { "-g " + c-wasm-flags-debug } else { c-wasm-flags-release } }} -flto {{ c-wasm-flags }} -o build/{{ target }}/{{ name }}-wasi-libc.wasm {{ wasm-samples }}/{{ name }}.c
    cargo run {{ if features =~ "gdb" { "--features gdb" } else { "" } }} -p hyperlight-wasm-aot compile {{ if features =~ "gdb" { "--debug" } else { "" } }} {{ if features =~ "wasmtime_latest" { "--wasmtime-version latest" } else { "" } }} build/{{ target }}/{{ name }}-wasi-libc.wasm build/{{ target }}/{{ name }}.aot

[private]
build-c-component name target features:
    wit-bindgen c {{ wasm-samples }}/components/{{ name }}.wit --out-dir {{ wasm-samples }}/components/bindings
    {{ wasi-clang }} --target=wasm32-wasip2 {{ if target == "debug" { c-wasm-flags-debug } else { c-wasm-flags-release } }} {{ c-wasm-flags }} -o build/{{ target }}/{{ name }}-p2.wasm {{ wasm-samples }}/components/{{ name }}.c {{ wasm-samples }}/components/bindings/{{ name }}.c {{ wasm-samples }}/components/bindings/{{ name }}_component_type.o
    cargo run {{ if features =~ "gdb" { "--features gdb" } else { "" } }} -p hyperlight-wasm-aot compile {{ if features =~ "gdb" { "--debug" } else { "" } }} {{ if features =~ "wasmtime_latest" { "--wasmtime-version latest" } else { "" } }} --component build/{{ target }}/{{ name }}-p2.wasm build/{{ target }}/{{ name }}.aot
