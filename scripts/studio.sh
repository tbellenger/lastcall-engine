#!/usr/bin/env bash
# Build Studio's wasm and start the editor.
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/build-studio.sh
cd studio
if [[ ! -d node_modules ]]; then
  npm install
fi
npm run studio
