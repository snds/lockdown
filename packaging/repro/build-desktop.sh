#!/usr/bin/env bash
# Builds the Linux desktop packages (deb, rpm) reproducibly
# (docs/plan/29-release-polish.md §1), and the release binaries the Flatpak
# packs. Everything that reaches the bytes is pinned:
#
# - the build system: ubuntu:22.04 by digest (glibc 2.35, so the packages
#   run on Ubuntu 22.04 and later, D279), its packages from
#   snapshot.ubuntu.com at a fixed time, so WebKitGTK's headers, the C
#   compiler, the linker and zlib are the same on every runner;
# - the tools: Rust (an exact release through a checksummed rustup-init),
#   Node (an exact release, checksummed), pnpm (packageManager), the Tauri
#   CLI and every JS package (the lock file), every crate (Cargo.lock);
# - the paths: the checkout at /src and the cargo home remapped out of the
#   binaries (--remap-path-prefix);
# - the times: Tauri's bundler stamps the deb and rpm with the build's own
#   clock and the file system's order, so normalize-package.py rewrites
#   both to SOURCE_DATE_EPOCH (the commit's time) and sorted order.
#
#   packaging/repro/build-desktop.sh <out dir>
#
# Writes <out>/lockdown_<v>_amd64.deb, <out>/lockdown-<v>-1.x86_64.rpm,
# <out>/stage/{lockdown-desktop,lockdown-native-host,icon-128.png} and
# <out>/SHA256SUMS.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$(mkdir -p "${1:?usage: build-desktop.sh <out dir>}" && cd "$1" && pwd)"
epoch="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}"

image="ubuntu:22.04@sha256:0a9887931b0e38af2e3487ed39e29436b0535c4f3f52e57745baaf79d1898956"
snapshot="20261015T000000Z"
rust="1.97.0"
rustup="1.29.1"
rustup_sha256="dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71"
node="22.23.3"
node_sha256="df450af89261115ef9f9e3830c3eeb2cc9213b63c720b1af623cb5dcbe2e02de"

docker run --rm \
  -v "$root:/src" -v "$out:/out" -w /src \
  -e SOURCE_DATE_EPOCH="$epoch" -e SNAPSHOT="$snapshot" \
  -e RUST="$rust" -e RUSTUP="$rustup" -e RUSTUP_SHA256="$rustup_sha256" \
  -e NODE="$node" -e NODE_SHA256="$node_sha256" \
  "$image" bash /src/packaging/repro/desktop-container.sh
# The container ran as root; hand the files back.
docker run --rm -v "$out:/out" -v "$root:/src" "$image" \
  chown -R "$(id -u):$(id -g)" /out /src/target /src/apps/desktop/node_modules /src/apps/desktop/dist /src/apps/desktop/src-tauri/binaries
(cd "$out" && cat SHA256SUMS)
