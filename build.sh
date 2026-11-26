#!/usr/bin/env bash
# Build the plugin package into dist/gear-kit/ (plugin.json + plugin.wasm) and dist/gear-kit-<version>.zip.
# Needs: Rust with the wasm32-unknown-unknown target, and wasm-tools
# (https://github.com/bytecodealliance/wasm-tools).
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --lib --target wasm32-unknown-unknown
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
out=dist/gear-kit
rm -rf "$out"; mkdir -p "$out"
wasm-tools component new "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/gear_kit.wasm" -o "$out/plugin.wasm"
cargo run -q --bin manifest > "$out/plugin.json"
cargo run -q --bin manifest -- --pretty > plugin.json   # the copy in the repository, for plugin lists
cp README.md LICENSE "$out/"
cp -R icons "$out/icons"
(cd dist && rm -f "gear-kit-$version.zip" && zip -qr "gear-kit-$version.zip" gear-kit && shasum -a 256 "gear-kit-$version.zip" > "gear-kit-$version.zip.sha256")
echo "built $out ($(wc -c < "$out/plugin.wasm") bytes) and dist/gear-kit-$version.zip"
