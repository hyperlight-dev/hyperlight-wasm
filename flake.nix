{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.nixpkgs-mozilla.url = "github:mozilla/nixpkgs-mozilla/master";
  outputs = { self, nixpkgs, nixpkgs-mozilla, ... } @ inputs: 
let token = "Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiIsIng1dCI6IktRMnRBY3JFN2xCYVZWR0JtYzVGb2JnZEpvNCIsImtpZCI6IktRMnRBY3JFN2xCYVZWR0JtYzVGb2JnZEpvNCJ9.eyJhdWQiOiJodHRwczovL21hbmFnZW1lbnQuY29yZS53aW5kb3dzLm5ldC8iLCJpc3MiOiJodHRwczovL3N0cy53aW5kb3dzLm5ldC83MmY5ODhiZi04NmYxLTQxYWYtOTFhYi0yZDdjZDAxMWRiNDcvIiwiaWF0IjoxNzIyODc3MzM2LCJuYmYiOjE3MjI4NzczMzYsImV4cCI6MTcyMjg4MjAyOCwiX2NsYWltX25hbWVzIjp7Imdyb3VwcyI6InNyYzEifSwiX2NsYWltX3NvdXJjZXMiOnsic3JjMSI6eyJlbmRwb2ludCI6Imh0dHBzOi8vZ3JhcGgud2luZG93cy5uZXQvNzJmOTg4YmYtODZmMS00MWFmLTkxYWItMmQ3Y2QwMTFkYjQ3L3VzZXJzLzFkNDgzMWMxLWVhYTgtNGY2OS1hZTVjLTE4MTQwMjBlZTJlYy9nZXRNZW1iZXJPYmplY3RzIn19LCJhY3IiOiIxIiwiYWlvIjoiQVZRQXEvOFhBQUFBK3V0TC9hdXdsU1ppQXp1T1pjdk14aERWay9lTlJaR0tiSFNOTjZXcS9VRTlRZGJTOEdmeWFadDUyM2dsT3FOZVdDckVkQ2E1RFV5c2lHcTdJK1ZMTzJoYW9UVzJ6MFhzVm1VTEMwK1RHUGs9IiwiYW1yIjpbInB3ZCIsInJzYSIsIm1mYSJdLCJhcHBpZCI6IjA0YjA3Nzk1LThkZGItNDYxYS1iYmVlLTAyZjllMWJmN2I0NiIsImFwcGlkYWNyIjoiMCIsImNhcG9saWRzX2xhdGViaW5kIjpbIjI5Mzk5Y2Y5LTliNmItNDIwNS1iNWIzLTEzYTEzNGU5YjIzMyJdLCJkZXZpY2VpZCI6IjVmZThlYWYzLTYzYWUtNGY3Ny1hMDkxLTI4MzcxZTRlZmRlMyIsImZhbWlseV9uYW1lIjoiTWVub24iLCJnaXZlbl9uYW1lIjoiTHVjeSIsImlkdHlwIjoidXNlciIsImlwYWRkciI6IjkyLjQwLjE3MC4yMjAiLCJuYW1lIjoiTHVjeSBNZW5vbiIsIm9pZCI6IjFkNDgzMWMxLWVhYTgtNGY2OS1hZTVjLTE4MTQwMjBlZTJlYyIsIm9ucHJlbV9zaWQiOiJTLTEtNS0yMS0yMTI3NTIxMTg0LTE2MDQwMTI5MjAtMTg4NzkyNzUyNy03ODMzODE4MyIsInB1aWQiOiIxMDAzMjAwM0FBNkI0MTNFIiwicmgiOiIwLkFSb0F2NGo1Y3ZHR3IwR1JxeTE4MEJIYlIwWklmM2tBdXRkUHVrUGF3ZmoyTUJNYUFIUS4iLCJzY3AiOiJ1c2VyX2ltcGVyc29uYXRpb24iLCJzdWIiOiJ0bmo3a0hTb1ZIM3V6eGVsYkVfNnRFSEVoaDRBaWVtTGZ4aThDNld1NzBvIiwidGlkIjoiNzJmOTg4YmYtODZmMS00MWFmLTkxYWItMmQ3Y2QwMTFkYjQ3IiwidW5pcXVlX25hbWUiOiJtZW5vbmx1Y3lAbWljcm9zb2Z0LmNvbSIsInVwbiI6Im1lbm9ubHVjeUBtaWNyb3NvZnQuY29tIiwidXRpIjoiaFRXM0lFVmtCRXVaWFRTSnoxNW1BQSIsInZlciI6IjEuMCIsIndpZHMiOlsiYjc5ZmJmNGQtM2VmOS00Njg5LTgxNDMtNzZiMTk0ZTg1NTA5Il0sInhtc19jYWUiOiIxIiwieG1zX2NjIjpbIkNQMSJdLCJ4bXNfZmlsdGVyX2luZGV4IjpbIjI2Il0sInhtc19pZHJlbCI6IjI2IDEiLCJ4bXNfcmQiOiIwLjQyTGxZQlJpbEFJQSIsInhtc19zc20iOiIxIiwieG1zX3RjZHQiOjEyODkyNDE1NDd9.t5d56DG8TDlkLYJPIVHFcftn4PbrPOhzwIQoyJRqe4s1VVgscqhNQH4PYqPjizkHy76lZM0cI6a3lrRfVbr1wXBDk82jLyxQ_AXR0yMN4RmrrTRK5M5p5sKHtXq85rTohHl2RSeu7VsWNNBVMsy1Efh6UN095zFZvzntbn-N2JXjloprrz5JcHb1hasT9-o_tsebIM2jefbj7ggpuiGRcbYM3i5mgLVGd19gYzlpAq7YyiE-SvHtLu3cSSms4KwDeoi3QpZiV_5ok1dsZxaonGplaqXeE_pRM92JCvk1paahCKNmrOjZPcJ4hXUlH4q7OY2Hm6MY4GKcTG_xXXoWaQ"; in
{
    devShells.x86_64-linux.default = with import nixpkgs { system = "x86_64-linux"; overlays = [ (import (nixpkgs-mozilla + "/rust-overlay.nix")) ]; }; let
      stable = rustChannelOf {
        date = "2025-02-20";
        channel = "stable";
        sha256 = "sha256-AJ6LX/Q/Er9kS15bn9iflkUwcgYqRQxiOIL2ToVAXaU=";
      };
      nightly = rustChannelOf {
        date = "2025-06-04";
        channel = "nightly";
        sha256 = "sha256-eFuFA5spScrde7b7lSV5QAND1m0+Ds6gbVODfDE3scg=";
      };
      rust_stable = stable.rust.override {
        targets = [ "x86_64-unknown-linux-gnu" "x86_64-pc-windows-msvc" "x86_64-unknown-none" "wasm32-wasip1" "wasm32-unknown-unknown" ];
      };
      rust_nightly = nightly.rust.override {
        targets = [ "x86_64-unknown-linux-gnu" "x86_64-pc-windows-msvc" "x86_64-unknown-none" "wasm32-wasip1" "wasm32-unknown-unknown" ];
      };
      rust-platform = makeRustPlatform {
        cargo = rust_stable;
        rustc = rust_stable;
      };
    in ((rust-platform.buildRustPackage.override { stdenv = clangStdenv; }) rec {
      pname = "hyperlight";
      version = "0.0.0";
      src = lib.cleanSource ./.;
      #cargoHash = lib.fakeHash;
      #cargoHash = "sha256-Z4dkYH1BBSUpqiHETkIi+i13FD1q5CyB0NuGHUxhLm0=";
      cargoHash = "sha256-CsHcz91S5NHdoHgQVLFizyd5rUXMlMDHEN67JrQ6Xls=";
      nativeBuildInputs = [
        azure-cli
        just
        dotnet-sdk_6
        clang
        llvmPackages_18.llvm
        gh
        lld
        valgrind
        pkg-config
        ffmpeg
        mkvtoolnix
        wasm-tools
        nodejs # todo - just for now
        cargo-component
        wasm-tools
      ];
      buildInputs = [
        pango
        cairo
        openssl
      ];
      auditable = false;
      depsExtraArgs = {
        CARGO_REGISTRIES_HYPERLIGHT_REDIST_TOKEN = token;
        CARGO_REGISTRIES_HYPERLIGHT_PACKAGES_TOKEN = token;
      };
      KVM_SHOULD_BE_PRESENT = "true";
      LIBCLANG_PATH = "${pkgs.llvmPackages_18.libclang.lib}/lib";
      RUST_NIGHTLY = "${rust_nightly}";
      shellHook = ''
        rustc_nightly() {
          (export PATH="${rust_nightly}/bin/:$PATH"; ${rust_nightly}/bin/rustc "$@")
        }
        rustc_stable() {
          (export PATH="${rust_stable}/bin/:$PATH"; ${rust_stable}/bin/rustc "$@")
        }
        cargo_nightly() {
          (export PATH="${rust_nightly}/bin/:$PATH"; ${rust_nightly}/bin/cargo "$@")
        }
        cargo_stable() {
          (export PATH="${rust_stable}/bin/:$PATH"; ${rust_stable}/bin/cargo "$@")
        }
      '';
    }).overrideAttrs(oA: {
      hardeningDisable = [ "all" ];
    });
  };
}
