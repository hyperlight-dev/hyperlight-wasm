# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).


## [Prerelease] - Unreleased

### Added

* Add `SandboxBuilder::with_interrupt_vcpu_sigrtmin_offset` to change the signal that interrupts a guest on Linux.
* Experimental: add the `experimental_runtime_file` feature, with `SandboxBuilder::with_runtime_file` to load the guest runtime from a file. Snapshots of such a sandbox cannot be saved.
* Experimental: add the `hyperlight-wasm-capi` crate, a C API that calls component functions with arguments and results in Hyperlight's component wire format. It is not published.
* Experimental: add the `Hyperlight.Wasm` and `Hyperlight.Wasm.Build` .NET packages, which run components from .NET with typed C# bindings generated from WIT at build time. See `dotnet/README.md`. They are not published.

### Changed

* Update Hyperlight dependencies to 0.19.0.
* Change component function names to `<instance>#<function>`. Generate host bindings with `hyperlight-component-macro` 0.19.

### Removed

* Remove `SandboxBuilder::with_guest_heap_size`. The guest heap uses scratch memory, so size it with `with_guest_scratch_size`.

## [v0.16.0] - 2026-11

### Added

* Add `Snapshot::save`, `Snapshot::load` and `Snapshot::checked_load` to save snapshots to disk and load them in another process. (#566)
* Add `SandboxBuilder::from_snapshot` to create a sandbox from a snapshot. (#566)
* Add `WasmSandbox::snapshot` and `WasmSandbox::restore`. (#566)
* Add `SandboxBuilder` setters for the host-to-guest and guest-to-host buffers: `with_h2g_pool_pages`, `with_h2g_queue_size`, `with_h2g_buffer_size` and their `g2h` counterparts. (#561)
* Support `Vec<Bytes>` arguments and results in module guest and host function calls. (#561)
* Support aarch64 hosts and AOT compilation of aarch64 guests. (#562)

### Changed

* Update Hyperlight dependencies to 0.18.0. (#561)
* Change `LoadedWasmSandbox::snapshot`, `LoadedWasmSandbox::restore` and `WasmSandbox::load_from_snapshot` to use `Snapshot<LoadedWasmSandbox>` instead of `Arc<Snapshot>`. (#566)
* Make `unload_module` return to the snapshot's runtime state after `LoadedWasmSandbox::restore` or `WasmSandbox::load_from_snapshot`. (#566)
* Count sandboxes created from snapshots in `sandbox_loads_total`. (#566)

### Deprecated

* Deprecate `SandboxBuilder::with_guest_input_buffer_size` and `SandboxBuilder::with_guest_output_buffer_size`. Use the `h2g` and `g2h` setters. (#561)

### Fixed

* Fix a native build embedding the Pulley runtime after a Pulley build. (#551)

## [v0.15.0] - 2026-08

### Changed

- Adapted component macro generation to the latest `hyperlight-component-util` APIs. (#520)
- Replaced the deprecated sandbox poison check with the current sandbox status API. (#520)
- Added support for selecting either the latest or LTS Wasmtime runtime, defaulting to LTS. (#301)

## [v0.14.0] - 2026-04

### Changed

- **BREAKING CHANGE:** Removed `SandboxBuilder::with_function_definition_size`. Host function definitions are now pushed to the guest at runtime load time instead of using a separate memory region. (#388)

### Added
- Added support for selecting a specific world from WIT files with multiple worlds using the `WIT_WORLD_NAME` environment variable (#202)
- Added `WasmSandbox::load_from_snapshot()` to create a loaded sandbox by restoring a snapshot taken from a `LoadedWasmSandbox`. (#437)
- Implemented Wasmtime memory image APIs by copying VA mappings and converting writable pages to CoW mappings. (#437)

## [v0.12.0] - 2025-12

### Added
- Added support for poisoned sandboxes. A poisoned sandbox is one that has encountered a fatal error during a previous execution. Once poisoned, any further attempts to use the sandbox will result in an error, preventing undefined behavior. To recover from a poisoned state, a new sandbox instance must be created, or the sandbox must be restored from a previously taken snapshot.

### Fixed
- Fixes Floating Point rounding issue, and uses cargo hyperlight for building wasm_runtime (#166)

### Changed
- Make sure hosts don't need hyperlight-host dependency (#291)

## v0.11.0 - 2025-11-20
- Add guest tracing support

## [v0.8.0] - 2025-08-13

### Added
- Added support for taking snapshots of memory using `LoadedWasmSandbox::snapshot`. A snapshot can be used to restore the memory state of the LoadedWasmSandbox to a specific point in time, using `LoadedWasmSandbox::restore`.

### Changed
- **BREAKING CHANGE:** `LoadedWasmSandbox::call_guest_function` no longer resets sandbox memory after the guest function call. If this old behavior is desired, manually call `LoadedWasmSandbox::snapshot` to take a snapshot of memory before calling guest function, and use `LoadedWasmSandbox::restore` after invoking the function.
- Updated to wasmtime v35.0.0

## [v0.7.0] - 2054-07-03

The Initial Hyperlight-wasm Release 🎉 


[Prerelease]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.16.0...HEAD>
[v0.16.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.15.0...v0.16.0>
[v0.15.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.14.0...v0.15.0>
[v0.14.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.12.0...v0.14.0>
[v0.12.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.11.0...v0.12.0>
[v0.11.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.10.0...v0.11.0>
[v0.10.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.9.0...v0.10.0>
[v0.9.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.8.0...v0.9.0>
[v0.8.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.7.0...v0.8.0>
[v0.7.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/releases/tag/v0.7.0>
