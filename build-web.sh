#!/usr/bin/env bash
# Builds the Rust engine to WebAssembly and drops it next to the static web app in web/.
# Serve the folder over http afterwards, e.g.:  python3 -m http.server -d web 8000
set -euo pipefail
cd "$(dirname "$0")"
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build --lib --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/drop_calc.wasm web/drop_calc.wasm
if command -v wasm-opt >/dev/null; then wasm-opt -O3 web/drop_calc.wasm -o web/drop_calc.wasm; fi
echo "Built web/drop_calc.wasm ($(du -h web/drop_calc.wasm | cut -f1))"
