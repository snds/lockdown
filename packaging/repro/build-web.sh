#!/usr/bin/env bash
# Builds the web vault's core (lockdown-web for wasm32-wasip1) reproducibly
# (docs/plan/24-release.md §5): ring's C code is compiled by clang, and a
# runner's own clang differs between Ubuntu releases, so the module is
# built inside the digest-pinned Rust image the server uses, with Debian
# bookworm's clang, the image's own Rust release (RUSTUP_TOOLCHAIN, not
# rust-toolchain.toml's moving "stable"), at the same paths and with
# remapped paths. Then
# apps/web/wasm/lockdown_web.wasm is in place for `pnpm build`.
#
#   packaging/repro/build-web.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
digest="$(sed -n 's/^ARG RUST_DIGEST=//p' "$root/Dockerfile")"
version="$(sed -n 's/^ARG RUST_VERSION=//p' "$root/Dockerfile")"
image="rust:${version}-slim-bookworm${digest}"
flags="--remap-path-prefix=/src=/lockdown --remap-path-prefix=/usr/local/cargo=/cargo"

docker run --rm -v "$root:/src" -w /src \
  -e RUSTFLAGS="$flags" -e CARGO_INCREMENTAL=0 -e CARGO_TERM_COLOR=never \
  "$image" sh -ec '
    # The image'"'"'s own Rust, not rust-toolchain.toml'"'"'s moving "stable".
    export RUSTUP_TOOLCHAIN="$RUST_VERSION"
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends clang llvm git ca-certificates > /dev/null
    rustup target add wasm32-wasip1
    bash apps/web/build-wasm.sh
  '
docker run --rm -v "$root:/src" "$image" chown -R "$(id -u):$(id -g)" /src/target /src/apps/web/wasm
sha256sum "$root/apps/web/wasm/lockdown_web.wasm"
