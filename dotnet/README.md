# Hyperlight.Wasm for .NET

> Warning: These packages are experimental. Their API and the generated C#
> can change in any release.

Run WebAssembly components in Hyperlight micro VMs from .NET, with typed bindings generated from WIT.

* `Hyperlight.Wasm` is the runtime library. It wraps the native `hyperlight-wasm-capi` library.
* `Hyperlight.Wasm.Build` generates C# from WIT at build time, which needs no Rust toolchain. It can also build the guest runtime and components. Visual Studio builds need Visual Studio 2026 or later.

Both target .NET 10 or later. The published packages hold native libraries for linux-x64, win-x64 and osx-arm64. Release versions end in `-preview`. Development builds of `main` are attached to the `dev-latest` GitHub release. To use one, download both `.nupkg` files into a directory and add it as a package source with `dotnet nuget add source <directory>`.

## Usage

```xml
<ItemGroup>
  <PackageReference Include="Hyperlight.Wasm" Version="0.17.0-preview" />
  <PackageReference Include="Hyperlight.Wasm.Build" Version="0.17.0-preview" />
  <HyperlightWit Include="wit/example.wit" World="example" />
</ItemGroup>
```

Each `HyperlightWit` item is a `.wit` file or directory. `World` selects a world when the package has several. Without it, the last world is used. `Namespace` sets the namespace of the generated types and defaults to the project's root namespace.

For a world `example` that imports `host` and exports `adder`, the build generates:

* `ExampleComponent`, with an `Adder` property for the export.
* `ExampleComponent.RegisterImports`, which registers an `IHost` implementation as host functions.
* C# types for the world's records, variants, enums and flags.

```csharp
var dir = AppContext.BaseDirectory;
using var sandbox = new WasmSandbox(Path.Combine(dir, "example-runtime"));
ExampleComponent.RegisterImports(sandbox, new Host());
sandbox.Load(Path.Combine(dir, "example.aot"));

var component = new ExampleComponent(sandbox);
uint sum = component.Adder.Add(1, 2);
```

`docs/component-wire-format.md` in hyperlight lists how WIT types map to C#.

## Guest runtime and components

A component needs a guest runtime built for its WIT world, because the runtime's guest bindings are generated when it is compiled. Components must be precompiled. The runtime, the precompiled component and the packages must come from the same hyperlight-wasm version.

Both steps use `hyperlight-wasm-aot`, which needs rustup and clang. Its runtime build is experimental, and uses Rust 1.94.1 unless `RUSTUP_TOOLCHAIN` is set. Install the packages' version without the `-preview` suffix:

```sh
cargo install --locked hyperlight-wasm-aot --version <version>
```

For development packages, the `dev-latest` release notes give the commands that install it from their commit. The `repository` element of a package's `.nuspec` also holds its commit.

### Building them with the project

Set `Runtime` on a `HyperlightWit` item and list components as `HyperlightComponent` items. The build then creates the runtime and `<name>.aot` in the output directory. The first runtime build takes several minutes. `HyperlightWasmAot` sets the tool's path, and `HyperlightWasmtimeVersion` selects `lts`, the default, or `latest`.

```xml
<HyperlightWit Include="wit/example.wit" World="example" Runtime="example-runtime" />
<HyperlightComponent Include="example.wasm" />
```

### Building them by hand

```sh
hyperlight-wasm-aot experimental-build-runtime example.wit example-runtime --world example
hyperlight-wasm-aot compile --component example.wasm example.aot
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
codesign -f -s - --entitlements macos-entitlements.plist bin/Release/net10.0/osx-arm64/publish/MyApp
```

`dotnet MyApp.dll` and `dotnet test` run inside the `dotnet` host, so they cannot create sandboxes on macOS. Test projects that build as executables, such as xUnit.net v3 ones, can be signed the same way and run directly.

## Surrogate process on Windows

Hyperlight runs a helper process, `hyperlight_surrogate_<hash>.exe`, for each sandbox on Windows. The first sandbox writes it to the directory of the running executable, which must be writable. For `dotnet MyApp.dll` that is the .NET installation directory, which needs administrator rights under Program Files. Publish the app with its own executable in a writable directory, or run it once as administrator.

## Development

```sh
just dotnet-test         # builds the native libraries and test component, then runs the tests
just dotnet-pack         # packs both packages for this machine's runtime identifier into dotnet/artifacts
```

`HyperlightNativeDir` overrides where the projects find the native libraries.
