#!/usr/bin/env bash
# INKFORGE — build the portable Windows ZIP from a Linux machine.
#
#   scripts/package-windows-cross.sh
#
# Cross-compiles with `cargo zigbuild` (zig's bundled MinGW toolchain) for
# x86_64-pc-windows-gnu, then stages the same layout as the MSVC script:
# dist/INKFORGE-<version>-windows-x86_64/{INKFORGE.exe,README.txt,LICENSE-MIT.txt}
# plus a matching .zip. The GNU build links the C runtime statically, so the
# executable is still portable: no Rust, MinGW or Visual C++ runtime to install.
#
# Needs: rustup target add x86_64-pc-windows-gnu, `zig` on PATH,
# `cargo install cargo-zigbuild`.
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || { echo "could not read the version from Cargo.toml" >&2; exit 1; }

target=x86_64-pc-windows-gnu
stage="dist/INKFORGE-$version-windows-x86_64"

echo "INKFORGE $version — building $target release"
cargo zigbuild --release --target "$target"

exe="target/$target/release/inkforge.exe"
[ -f "$exe" ] || { echo "expected $exe to exist" >&2; exit 1; }

rm -rf "$stage"
mkdir -p "$stage"
cp "$exe" "$stage/INKFORGE.exe"
cp packaging/README.txt "$stage/README.txt"
cp LICENSE-MIT "$stage/LICENSE-MIT.txt"

rm -f "$stage.zip"
(cd dist && zip -q -r "$(basename "$stage").zip" "$(basename "$stage")")

echo
echo "Portable build: $stage/INKFORGE.exe"
echo "Archive:        $stage.zip"
du -h "$stage.zip"
