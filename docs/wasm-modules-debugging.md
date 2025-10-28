# Debugging WebAssembly Modules

This guide provides an overview of the methods and tools available for debugging native Wasm modules in the Hyperlight-Wasm environment. Effective debugging is crucial for identifying and resolving issues in Wasm modules, ensuring they function correctly within the host application.

## Building with Debug Information

To facilitate debugging, it is recommended to compile your Wasm modules with debug information included. This can be achieved by using the appropriate flags during the compilation process.
For example, when using `wasi-sdk/clang`, you can include the `-g` flag to generate debug information and `-O0` to disable optimizations.
You can find examples of native Wasm modules in the repository under the `rust_wasm_samples`, `wasm_samples` and `component_sample` directories.

Next, ensure that the pre-compiled Wasm modules are built with debug information as well. To do this, you can use the `hyperlight-wasm-aot` tool with the `--debug` command argument:

```bash
cargo run -p hyperlight-wasm-aot compile --debug input.wasm output.aot
```


## Example of debugging a Wasm Module
We have a simple example of debugging a Wasm module in the [examples/guest-debugging](../src/hyperlight_wasm/examples/guest-debugging) directory.

This example demonstrates how to configure a Hyperlight Sandbox to enable debugging support for a Wasm module.

When running the example, it starts a GDB server on `localhost:8080`, allowing you to connect your debugger to this address.
It hangs waiting for a debugger to attach before proceeding with the execution.
You can use either GDB or LLDB to attach to the running process.
You can find a sample `launch.json` configuration for Visual Studio Code that sets up remote debugging for the Wasm module using both GDB and LLDB at [VSCode launch.json Configuration](#vscode-launchjson-configuration).

The json configuration asks for the path to the guest binary to debug, which you see printed as a `cargo:warning` when `hyperlight-wasm`, with `gdb` feature enabled, is built.
After you attach, you can set breakpoints, step through the code, and inspect variables in your wasm module as you would with any native application.

**NOTE**: The debugger stops at the entry point of the guest binary, which is the `hyperlight-wasm` runtime in this case, so you may need to set additional breakpoints in your Wasm module code to effectively debug it.

### Visual Studio Code `launch.json` Configuration

```json

{
  "inputs": [
    {
      "id": "program",
      "type": "promptString",
      "default": "x64/debug/wasm_runtime",
      "description": "Path to the program to debug"
    },
  ],
  "configurations": [
    {
      "name": "Remote GDB attach",
      "type": "cppdbg",
      "request": "launch",
      "program": "${input:program}",
      "args": [],
      "stopAtEntry": true,
      "hardwareBreakpoints": {
        "require": false,
        "limit": 4
      },
      "cwd": "${workspaceFolder}",
      "environment": [],
      "externalConsole": false,
      "MIMode": "gdb",
      "miDebuggerPath": "/usr/bin/gdb",
      "miDebuggerServerAddress": "localhost:8080",
      "setupCommands": [
        {
          "description": "Enable pretty-printing for gdb",
          "text": "-enable-pretty-printing",
          "ignoreFailures": true
        },
        {
          "description": "Set Disassembly Flavor to Intel",
          "text": "-gdb-set disassembly-flavor intel",
          "ignoreFailures": true
        }
      ]
    },
    {
      "name": "Remote LLDB attach",
      "type": "lldb",
      "request": "launch",
      "targetCreateCommands": [
        "target create ${input:program}"
      ],
      "processCreateCommands": [
        "gdb-remote localhost:8080"
      ]
    },
  ]
}
```
