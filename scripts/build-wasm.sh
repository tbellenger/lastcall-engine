#!/usr/bin/env bash
# Build the wasm-bindgen package consumed by the browser shell.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p lastcall-wasm --release --target wasm32-unknown-unknown
mkdir -p web/src/wasm
wasm-bindgen target/wasm32-unknown-unknown/release/lastcall_wasm.wasm \
  --out-dir web/src/wasm \
  --target web \
  --typescript
