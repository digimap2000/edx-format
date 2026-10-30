#!/bin/sh
# Builds the WebAssembly core and its JavaScript glue for Node.js and for bundlers.
# Needs: rustup target add wasm32-unknown-unknown; cargo install wasm-bindgen-cli --version <the one in Cargo.lock>
set -eu
cd "$(dirname "$0")"
CARGO_PROFILE_RELEASE_OPT_LEVEL=s CARGO_PROFILE_RELEASE_LTO=true CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  cargo build --release --locked -p edx-format-js --target wasm32-unknown-unknown
wasm=../../target/wasm32-unknown-unknown/release/edx_format_wasm.wasm
rm -rf pkg
wasm-bindgen "$wasm" --target nodejs --out-dir pkg/node --no-typescript --remove-name-section --remove-producers-section
wasm-bindgen "$wasm" --target bundler --out-dir pkg/bundler --no-typescript --remove-name-section --remove-producers-section
