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
cargo +1.94.0 run --manifest-path src/hyperlight_wasm/examples/js-component/Cargo.toml
```

The local `rust-toolchain.toml` pins this standalone example to Rust 1.94
because `componentize-qjs` currently requires Rust 1.94.

## WIT world

`wit/hello.wit` is the source WIT file. `wit/hello-world.wasm` is the
binary-encoded WIT world consumed by Hyperlight's component macros at compile
time through `WIT_WORLD`.

Regenerate it with:

```sh
wasm-tools component wit -w \
  -o src/hyperlight_wasm/examples/js-component/wit/hello-world.wasm \
  src/hyperlight_wasm/examples/js-component/wit/hello.wit
```

The generated `.aot` component is build output and is not checked in.
