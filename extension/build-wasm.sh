#!/usr/bin/env bash
# Builds `lockdown-wasm` for the browser and writes the wasm-bindgen output
# to extension/pkg/ (ES module, `--target web`, usable from a page, a worker
# or an MV3 offscreen document without a bundler).
#
# Needs: the `wasm32-unknown-unknown` target (`rustup target add
# wasm32-unknown-unknown`) and a `wasm-bindgen` CLI whose version equals the
# `wasm-bindgen` crate pinned in crates/wasm/Cargo.toml. The script checks
# both and says what to install when one is missing.
#
# `getrandom` 0.3 (reached through the crypto stack) only compiles for this
# target with `--cfg getrandom_backend="wasm_js"`; it is passed through the
# target-scoped variable so host builds and build scripts never see it.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
profile="${1:-release}"
out="$here/pkg"

want="$(sed -n 's/^wasm-bindgen = "=\([0-9.]*\)"/\1/p' "$root/crates/wasm/Cargo.toml")"
if [ -z "$want" ]; then
  echo "build-wasm: could not read the pinned wasm-bindgen version from crates/wasm/Cargo.toml" >&2
  exit 2
fi
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "build-wasm: wasm-bindgen CLI not found; install with: cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 2
fi
have="$(wasm-bindgen --version | awk '{print $2}')"
if [ "$have" != "$want" ]; then
  echo "build-wasm: wasm-bindgen CLI is $have but the crate is pinned to $want; install with: cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 2
fi
if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
  echo "build-wasm: target missing; run: rustup target add wasm32-unknown-unknown" >&2
  exit 2
fi

cd "$root"
export CARGO_NET_GIT_FETCH_WITH_CLI=true
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='--cfg getrandom_backend="wasm_js"'
if [ "$profile" = "release" ]; then
  cargo build -p lockdown-wasm --target wasm32-unknown-unknown --release
else
  cargo build -p lockdown-wasm --target wasm32-unknown-unknown
fi

rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --out-dir "$out" --out-name lockdown_wasm \
  "$root/target/wasm32-unknown-unknown/$profile/lockdown_wasm.wasm"

# wasm-opt is optional: smaller output when binaryen is installed, same
# behaviour when it is not.
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Os -o "$out/lockdown_wasm_bg.wasm" "$out/lockdown_wasm_bg.wasm"
fi

ls -la "$out"
