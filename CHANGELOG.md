# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).


## [Prerelease] - Unreleased

### Added
- Added support for selecting a specific world from WIT files with multiple worlds using the `WIT_WORLD_NAME` environment variable (#202)

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


[Prerelease]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.12.0..HEAD>
[v0.12.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.11.0...v0.12.0>
[v0.11.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.10.0...v0.11.0>
[v0.10.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.9.0...v0.10.0>
[v0.9.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.8.0...v0.9.0>
[v0.8.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.7.0...v0.8.0>
[v0.7.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/releases/tag/v0.7.0>
