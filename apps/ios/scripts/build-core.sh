#!/usr/bin/env bash
# Builds the Rust core for each given iOS target, generates the Swift bindings from the first one
# and packages every slice into SonoraRust.xcframework inside the sonora-core module.
# Usage: apps/ios/scripts/build-core.sh aarch64-apple-ios [aarch64-apple-ios-sim ...]
set -euo pipefail

[ "$#" -ge 1 ] || { echo "usage: $0 <target>..." >&2; exit 2; }

ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$ROOT"
MODULE=${MODULE:-apps/ios/modules/sonora-core/ios}

libs=()
for target in "$@"; do
  rustup target add "$target"
  cargo rustc --locked -p mobile --release --target "$target" --crate-type staticlib -- --print native-static-libs
  ls -la "target/$target/release/libmobile.a"
  libs+=("target/$target/release/libmobile.a")
done

cargo run --locked --release --manifest-path crates/mobile/bindgen/Cargo.toml -- \
  generate --library "${libs[0]}" --language swift --out-dir build/swift
ls -la build/swift
mkdir -p "$MODULE/generated"
cp build/swift/mobile.swift build/swift/mobileFFI.h "$MODULE/generated/"

rm -rf "$MODULE/rust/SonoraRust.xcframework"
args=()
for lib in "${libs[@]}"; do args+=(-library "$lib"); done
xcodebuild -create-xcframework "${args[@]}" -output "$MODULE/rust/SonoraRust.xcframework"
find "$MODULE/rust/SonoraRust.xcframework" -maxdepth 2
