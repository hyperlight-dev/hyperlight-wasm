# Hyperlight.Wasm for .NET

> **Warning:** These packages are experimental. Their API and the generated C#
> can change in any release.

Run WebAssembly components in Hyperlight micro VMs from .NET, with typed bindings generated from WIT.

* `Hyperlight.Wasm` is the runtime library. It wraps the native `hyperlight-wasm-capi` library.
* `Hyperlight.Wasm.Build` generates C# from WIT at build time. It needs no Rust toolchain.

Both target .NET 8 or later. The packages hold native libraries for the runtime identifier they were packed on.

## Usage

```xml
<ItemGroup>
  <PackageReference Include="Hyperlight.Wasm" Version="0.16.0-preview" />
  <PackageReference Include="Hyperlight.Wasm.Build" Version="0.16.0-preview" />
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

A component needs a guest runtime built for its WIT world, because the runtime's guest bindings are generated when it is compiled. Building a runtime needs Rust. From a hyperlight-wasm checkout of the same version as the packages:

```sh
wasm-tools component wit example.wit -w -o example-world.wasm
WIT_WORLD=example-world.wasm cargo hyperlight build --release \
    --manifest-path src/hyperlight_wasm_runtime/Cargo.toml --target-dir target/example-runtime
```

Precompile the component with `hyperlight-wasm-aot compile --component`. The runtime, the AOT tool and the packages must come from the same hyperlight-wasm version.

### Building the runtime without a checkout

Experimental: the `experimental-build-runtime` subcommand of `hyperlight-wasm-aot` builds the runtime without a hyperlight-wasm checkout. It needs rustup and clang, and builds with Rust 1.94.1 unless `RUSTUP_TOOLCHAIN` is set:

```sh
cargo install --locked hyperlight-wasm-aot
hyperlight-wasm-aot experimental-build-runtime example.wit example-runtime --world example
```

## Sandboxes

* Register host functions, then load a component, then call it.
* Calls are serialized. Host functions run on the calling thread and cannot call back into their own sandbox.
* An exception thrown by a host function fails the guest call. The `HyperlightException` holds it as `InnerException`.
* A trap or interrupted call poisons the sandbox. Restore a snapshot taken with `CreateSnapshot` to use it again.
* `Call` takes a `CancellationToken`. Cancelling interrupts the guest. `CreateInterruptHandle` returns a handle that does the same from any thread.

## Hypervisor entitlement on macOS

macOS lets a process use Hypervisor.framework only when its executable is signed with the `com.apple.security.hypervisor` entitlement. Publish the app as a single file and sign that executable with an entitlements file such as `dev/macos-entitlements.plist`:

```sh
dotnet publish -c Release -r osx-arm64 -p:PublishSingleFile=true
codesign -f -s - --entitlements macos-entitlements.plist bin/Release/net8.0/osx-arm64/publish/MyApp
```

`dotnet MyApp.dll` and `dotnet test` run inside the `dotnet` host, so they cannot create sandboxes on macOS.

## Surrogate process on Windows

Hyperlight runs a helper process, `hyperlight_surrogate_<hash>.exe`, for each sandbox on Windows. The first sandbox writes it to the directory of the running executable, which must be writable. For `dotnet MyApp.dll` that is the .NET installation directory, which needs administrator rights under Program Files. Publish the app with its own executable in a writable directory, or run it once as administrator.

## Development

```sh
just dotnet-test   # builds the native libraries, a test runtime and component, then runs the tests
just dotnet-pack   # packs both packages into dotnet/artifacts
```

`HyperlightNativeDir` overrides where the projects find the native libraries.
