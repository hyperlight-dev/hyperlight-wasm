#!/usr/bin/env bash
set -Eeuo pipefail


codesign -f -s - --entitlements "$(dirname "$0")/macos-entitlements.plist" "$1"
export DYLD_LIBRARY_PATH=$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/lib/
exec "$@"
