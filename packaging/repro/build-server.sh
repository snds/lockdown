#!/usr/bin/env bash
# Builds `lockdown-server` and the `lockdown` CLI for x86_64 Linux,
# reproducibly (docs/plan/24-release.md §5): inside the same pinned Rust
# image the Dockerfile builds in, at the same paths (/src, /src/target,
# /usr/local/cargo), with the same --remap-path-prefix flags, the lock file
# and the release profile (codegen-units = 1). The server is built on its
# own, exactly as the Dockerfile does, so its bytes equal the image's copy.
# The compiler is the image's own Rust release (RUSTUP_TOOLCHAIN), so the
# digest pins it.
#
#   packaging/repro/build-server.sh <out dir>
#
# Writes <out>/lockdown-server, <out>/lockdown and <out>/SHA256SUMS.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$(mkdir -p "${1:?usage: build-server.sh <out dir>}" && cd "$1" && pwd)"
digest="$(sed -n 's/^ARG RUST_DIGEST=//p' "$root/Dockerfile")"
version="$(sed -n 's/^ARG RUST_VERSION=//p' "$root/Dockerfile")"
image="rust:${version}-slim-bookworm${digest}"
flags="--remap-path-prefix=/src=/lockdown --remap-path-prefix=/usr/local/cargo=/cargo"

docker run --rm \
  -v "$root:/src" -v "$out:/out" -w /src \
  -e RUSTFLAGS="$flags" -e CARGO_INCREMENTAL=0 -e CARGO_TERM_COLOR=never \
  "$image" sh -ec '
    # The image'"'"'s own Rust, as the Dockerfile uses, not rust-toolchain.toml'"'"'s
    # moving "stable".
    export RUSTUP_TOOLCHAIN="$RUST_VERSION"
    cargo build --locked --release -p lockdown-server
    cargo build --locked --release -p lockdown-cli
    install -m 0755 target/release/lockdown-server /out/lockdown-server
    install -m 0755 target/release/lockdown /out/lockdown
  '
# The container ran as root; hand the files back.
docker run --rm -v "$out:/out" "$image" chown -R "$(id -u):$(id -g)" /out
docker run --rm -v "$root:/src" "$image" chown -R "$(id -u):$(id -g)" /src/target
(cd "$out" && sha256sum lockdown-server lockdown > SHA256SUMS && cat SHA256SUMS)
