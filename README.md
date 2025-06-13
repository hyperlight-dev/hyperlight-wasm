# Hyperlight-Wasm - Run Wasm Modules in a Virtual Machine backed sandbox

_Hyperlight-Wasm_ is a component that enables Wasm Modules to be run inside lightweight  Virtual Machine backed Sandbox. Its purpose is to enable applications to  safely run untrusted or third party Wasm code within a VM with very low latency/overhead. It is built on top of [Hyperlight](https://github.com/hyperlight-dev/hyperlight).

Hyperlight-Wasm currently supports running applications using either the [Windows Hypervisor Platform](https://docs.microsoft.com/en-us/virtualization/api/#windows-hypervisor-platform) on Windows, [KVM](https://www.linux-kvm.org/page/Main_Page) on Linux or [/dev/mshv](https://github.com/rust-vmm/mshv).

WARNING: This is experimental code. It is not considered production-grade by its developers, neither is it "supported" software.

This repo contains hyperlight-wasm along with a couple of sample Wasm modules and an example host application that can be used to either test or try it out.

This document contains instructions on  building `Hyperlight-Wasm`. For usage instructions, please see [RustDev.md](./RustDev.md).

## Prerequisites

### Windows

Make sure the following are installed:

1. [Rust](https://www.rust-lang.org/tools/install)
1. [just](https://github.com/casey/just).  is used as a command runner  `cargo install just`. Do not install `just` with Chocolately because it installs an older incompatible version, make sure to use at least version 1.5.0 if not installed through cargo.
1. [pwsh](https://github.com/PowerShell/PowerShell)
1. [git](https://gitforwindows.org/)
1. [GitHub CLI](https://github.com/cli/cli#installation)

Ensure that Windows Hypervisor Platform is enabled:

```PowerShell
Enable-WindowsOptionalFeature -Online -FeatureName HyperVisorPlatform
```

### Linux

#### Ubuntu/KVM

Make sure the following are installed:

1. build-essential: `sudo apt install build-essential`
1. [Rust](https://www.rust-lang.org/tools/install) `curl --proto '=https' --tlsv1.3 https://sh.rustup.rs -sSf | sh`
1. [just](https://github.com/casey/just).  is used as a command runner  `cargo install just`. Do not install `just` through a package manager as it may install an older incompatible version, make sure to use at least version 1.5.0 if not installed through cargo.
1. [GitHub CLI](https://github.com/cli/cli#installation)

Ensure that KVM is enabled: On an Azure VM using a size that supports nested virtualisation:

`sudo apt install cpu-checker && kvm-ok`

If KVM is installed and available you should see the output

``` console
INFO: /dev/kvm exists
KVM acceleration can be used
```

You should also add your user to the kvm group: `sudo adduser $USER kvm`

## Building

NOTE: Ensure that you use version 1.85.0 of rust toolchain.

```Console
rustup install 1.85.0
rustup default 1.85.0
```

Now you can build the Rust Wasm library:

```Console
just build
```

And the example wasm modules used by the tests:
```Console
just build-wasm-examples
just build-rust-wasm-examples
```

Then you can test the Rust Wasm library and run the example:

```Console
just test
cargo run --example helloworld
```

## Component Model support

Hyperlight-Wasm has experimental support for running WebAssembly
Component Model components, rather than core wasm modules.  In this
mode, set the `WIT_WORLD` environment variable to point to
a binary encoding of a component type (e.g. the result of running
`wasm-tools component wit -w -o /path/to/output.wasm
/path/to/input.wit`), which will ensure that the resultant library
includes a guest binary that is specialised to load components with
that type. Then, use `hyperlight_component_macro::host_bindgen!()` to
generate bindings from the same component type in the host.  For a
complete (albeit small) example of this, see [this
example](https://aka.ms/hyperlight-wasm-sockets-sample).

## Code of Conduct

This project has adopted the [Microsoft Open Source Code of
Conduct](https://opensource.microsoft.com/codeofconduct/).

For more information see the [Code of Conduct
FAQ](https://opensource.microsoft.com/codeofconduct/faq/) or contact
[opencode@microsoft.com](mailto:opencode@microsoft.com) with any additional questions or comments.

## FOSSA Status
[![FOSSA License Status](https://app.fossa.com/api/projects/git%2Bgithub.com%2Fhyperlight-dev%2Fhyperlight-wasm.svg?type=shield&issueType=license)](https://app.fossa.com/projects/git%2Bgithub.com%2Fhyperlight-dev%2Fhyperlight-wasm?ref=badge_shield&issueType=license)
[![FOSSA Security Status](https://app.fossa.com/api/projects/git%2Bgithub.com%2Fhyperlight-dev%2Fhyperlight-wasm.svg?type=shield&issueType=security)](https://app.fossa.com/projects/git%2Bgithub.com%2Fhyperlight-dev%2Fhyperlight-wasm?ref=badge_shield&issueType=security)
