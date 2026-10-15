# Hyperlight.Wasm for .NET

Run WebAssembly components in Hyperlight micro VMs from .NET, with typed bindings generated from WIT.

* `Hyperlight.Wasm` is the runtime library. It wraps the native `hyperlight-wasm-capi` library.
* `Hyperlight.Wasm.Build` generates C# from WIT at build time. It needs no Rust toolchain.

Both target .NET 8 or later. The packages hold native libraries for the runtime identifier they were packed on.

## Usage

```xml
<ItemGroup>
  <PackageReference Include="Hyperlight.Wasm" Version="0.15.0" />
  <PackageReference Include="Hyperlight.Wasm.Build" Version="0.15.0" />
  <HyperlightWit Include="wit/example.wit" World="example" />
</ItemGroup>
```

Each `HyperlightWit` item is a `.wit` file or directory. `World` selects a world when the package has several. Without it, the last world is used. `Namespace` sets the namespace of the generated types and defaults to the project's root namespace.

For a world `example` that imports `host` and exports `adder`, the build generates:

* `ExampleComponent`, with an `Adder` property for the export.
* `ExampleComponent.RegisterImports`, which registers an `IHost` implementation as host functions.
* C# types for the world's records, variants, enums and flags.

```csharp
using var sandbox = new WasmSandbox(new SandboxOptions
{
    RuntimePath = "example-runtime",
});
ExampleComponent.RegisterImports(sandbox, new Host());
sandbox.Load("example.aot");

var component = new ExampleComponent(sandbox);
uint sum = component.Adder.Add(1, 2);
```

`docs/component-wire-format.md` in hyperlight lists how WIT types map to C#.

## Guest runtime and components

A component needs a guest runtime built for its WIT world:

```sh
WIT_WORLD=example-world.wasm cargo hyperlight build --release \
    --manifest-path src/hyperlight_wasm_runtime/Cargo.toml --target-dir target/example-runtime
```

`example-world.wasm` comes from `wasm-tools component wit example.wit -w -o example-world.wasm`. Precompile the component with `hyperlight-wasm-aot compile --component`. The runtime, the AOT tool and the packages must come from the same hyperlight-wasm version.

## Sandboxes

* Register host functions, then load a component, then call it.
* Calls are serialized. Host functions run on the calling thread and cannot call back into their own sandbox.
* An exception thrown by a host function fails the guest call. The `HyperlightException` holds it as `InnerException`.
* A trap or interrupted call poisons the sandbox. Restore a snapshot taken with `CreateSnapshot` to use it again.
* `Call` takes a `CancellationToken`. Cancelling interrupts the guest. `CreateInterruptHandle` returns a handle that does the same from any thread.

## Interrupt signal on Linux

Hyperlight interrupts a running guest with the signal `SIGRTMIN + InterruptSignalOffset`. The .NET runtime suspends threads for garbage collection with `SIGRTMIN`, so the offset defaults to 1. With offset 0 a garbage collection can hang.

## Development

```sh
just dotnet-test   # builds the native libraries, a test runtime and component, then runs the tests
just dotnet-pack   # packs both packages into dotnet/artifacts
```

`HyperlightNativeDir` overrides where the projects find the native libraries.
