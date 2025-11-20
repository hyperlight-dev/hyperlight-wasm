# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).


## [Prerelease] - Unreleased

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


[Prerelease]: <https://github.com/hyperlight-dev/hyperlight/compare/v0.11.0..HEAD>
[v0.11.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.10.0...v0.11.0>
[v0.10.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.9.0...v0.10.0>
[v0.9.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.8.0...v0.9.0>
[v0.8.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/compare/v0.7.0...v0.8.0>
[v0.7.0]: <https://github.com/hyperlight-dev/hyperlight-wasm/releases/tag/v0.7.0>
