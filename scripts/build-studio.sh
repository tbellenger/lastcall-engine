#!/usr/bin/env bash
# Build the Studio wasm package consumed by studio/.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p lastcall-studio --release --target wasm32-unknown-unknown
mkdir -p studio/src/wasm
wasm-bindgen target/wasm32-unknown-unknown/release/lastcall_studio.wasm \
  --out-dir studio/src/wasm \
  --target web \
  --typescript
