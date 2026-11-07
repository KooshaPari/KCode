#!/usr/bin/env bash
# Populate the platform npm packages from jcode release tarballs.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <release-assets-directory>" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
assets="$(cd "$1" && pwd)"

prepare() {
  local package="$1" archive="$2" archived_binary="$3" installed_binary="$4"
  local package_dir="$repo_root/sdk/npm/$package"
  rm -rf "$package_dir/bin"
  mkdir -p "$package_dir/bin"
  tar -xzf "$assets/$archive" -C "$package_dir/bin"
  mv "$package_dir/bin/$archived_binary" "$package_dir/bin/$installed_binary"
  chmod +x "$package_dir/bin/$installed_binary"
}

# The 4th argument is the filename the npm payload ships as, and it MUST be
# `kcode`/`kcode.exe`. sdk/typescript's assertKcodePath (src/product.ts:8-19)
# rejects any path component matching /^(?:jcode(?:[.-].*)?|\.jcode.*)$/i, so a
# payload named `jcode` would be refused by the SDK that shipped it. The
# 2nd/3rd arguments stay `kcode-*` because those are the release asset names
# produced by release.yml, not the payload name.
prepare linux-x64 kcode-linux-x86_64.tar.gz kcode-linux-x86_64 kcode
prepare linux-arm64 kcode-linux-aarch64.tar.gz kcode-linux-aarch64 kcode
prepare darwin-x64 kcode-macos-x86_64.tar.gz kcode-macos-x86_64 kcode
prepare darwin-arm64 kcode-macos-aarch64.tar.gz kcode-macos-aarch64 kcode
prepare win32-x64 kcode-windows-x86_64.tar.gz kcode-windows-x86_64.exe kcode.exe
# Windows ARM64 is published only after the separate ARM64 runtime
# qualification (docs/KCODE_ISOLATION.md), so the asset is absent on an
# unqualified release and preparing this package would fail on a missing
# tarball. Skip it to keep the rest of the SDK publishable.
if [[ "${WINDOWS_ARM64_QUALIFIED:-}" == 'true' ]]; then
  prepare win32-arm64 kcode-windows-aarch64.tar.gz kcode-windows-aarch64.exe kcode.exe
else
  echo "skipping win32-arm64: WINDOWS_ARM64_QUALIFIED is not 'true'" >&2
fi
