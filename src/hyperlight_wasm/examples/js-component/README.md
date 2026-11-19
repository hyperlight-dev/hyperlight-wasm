# JavaScript Component Example

This example builds a JavaScript WebAssembly component with
[`componentize-qjs`](https://crates.io/crates/componentize-qjs) and runs it in
`hyperlight-wasm`.

The JavaScript guest demonstrates:

- calling an imported host `print` function
- exporting a WIT interface from JavaScript
- passing `list<string>` values
- returning WIT records
- converting a JavaScript `throw` into a WIT `result<_, string>` error

## Run

From this directory:

```sh
cargo run
```

or from the repository root:

```sh
(cd src/hyperlight_wasm/examples/js-component && cargo run)
```

The local `rust-toolchain.toml` uses the same Rust 1.94 toolchain as CI;
`componentize-qjs` currently requires Rust 1.94.
Run Cargo from the example directory so that it loads the WIT settings in
`.cargo/config.toml`.

The default uses the LTS Wasmtime runtime. To use the latest supported Wasmtime
runtime and its matching AOT compiler, run `cargo run --features wasmtime_latest`.

## WIT world

`wit/hello.wit` is used directly by `componentize-qjs`, the host bindings, and
the Hyperlight runtime bindings. `.cargo/config.toml` sets `WIT_WORLD` to this
text file and selects the `hello` world with `WIT_WORLD_NAME`; no compiled WIT
world needs to be generated or checked in.

The generated `.wasm` and `.aot` components are build output and are not checked in.

## Size-optimized runtime

The default build uses `componentize-qjs`'s `DefaultSync` runtime. To use its
`OptSizeSync` runtime instead, run from this directory:

```sh
COMPONENTIZE_QJS_OPT_SIZE=1 cargo run
```

The generated component is `target/debug/build/hyperlight-wasm-componentize-qjs-example-*/out/hello.component.wasm`.
