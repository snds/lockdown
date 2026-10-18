#!/usr/bin/env bash
# Builds the web vault's core (crates/web: lockdown-app for wasm32-wasip1)
# and copies the module to apps/web/wasm/, where the worker imports it and
# the Vite build bakes its SHA-384 into the worker (docs/plan/17-web-vault.md).
#
# Needs the `wasm32-wasip1` target (`rustup target add wasm32-wasip1`).
# No wasm-bindgen: the module's boundary is a plain C ABI and WASI.
#
# The dev profile is used with optimisation on and debug info off: the
# Argon2id stretching inside OPAQUE needs optimised code to finish in a
# second or two, and the shared target directory keeps one profile per
# target (these settings touch only the wasm32-wasip1 units).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-wasip1; then
  echo "build-wasm: target missing; run: rustup target add wasm32-wasip1" >&2
  exit 2
fi

cd "$root"
export CARGO_NET_GIT_FETCH_WITH_CLI=true
export CARGO_PROFILE_DEV_OPT_LEVEL=3
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=false
export CARGO_PROFILE_DEV_OVERFLOW_CHECKS=false
cargo build -p lockdown-web --target wasm32-wasip1

target_dir="$(cargo metadata --format-version 1 --no-deps 2>/dev/null \
  | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
if [ -z "$target_dir" ]; then
  echo "build-wasm: couldn't read cargo's target directory from 'cargo metadata'" >&2
  exit 2
fi
wasm="$target_dir/wasm32-wasip1/debug/lockdown_web.wasm"
if [ ! -f "$wasm" ]; then
  echo "build-wasm: built module not found at $wasm" >&2
  exit 2
fi

mkdir -p "$here/wasm"
cp "$wasm" "$here/wasm/lockdown_web.wasm"
ls -la "$here/wasm"
