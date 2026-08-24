#!/usr/bin/env bash
# Builds the Bloop WASM REPL and its JS bindings into www/pkg/, the same way
# CI does (see .github/workflows/rust.yml) so a local build produces exactly
# what gets deployed.
set -euo pipefail

cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown

if ! command -v wasm-pack >/dev/null 2>&1; then
    echo "wasm-pack not found; installing..."
    if ! curl -fsSL https://rustwasm.github.io/wasm-pack/installer/init.sh | sh; then
        echo "installer script unreachable, falling back to 'cargo install wasm-pack'"
        cargo install wasm-pack
    fi
fi

wasm-pack build --target web --out-dir www/pkg -- --no-default-features --features wasm

cat <<'MSG'

Built www/pkg/. ES module imports need http(s):// (not file://), so serve
www/ locally, e.g.:

  python3 -m http.server 8000 --directory www

then open http://localhost:8000/
MSG
