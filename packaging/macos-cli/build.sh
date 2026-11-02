#!/usr/bin/env bash
# Builds the `lockdown` command for a Mac (docs/plan/46-mac-cli-download.md):
# `lockdown-cli` for Apple silicon and for Intel, release profile, the lock
# file, no features, joined into one universal binary with lipo. The
# developer tools and `legacy-install` are only ever dev-dependencies of
# the command line, so a plain build never links them; release.yml proves
# it with packaging/devtools/check.sh absent.
#
#   packaging/macos-cli/build.sh <out dir>
#
# Writes <out>/lockdown, as the linker signed it (ad hoc, per slice).
# sign.sh signs the whole of it next.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$(mkdir -p "${1:?usage: build.sh <out dir>}" && cd "$1" && pwd)"
targets=(aarch64-apple-darwin x86_64-apple-darwin)

cd "$root"
args=()
for t in "${targets[@]}"; do args+=(--target "$t"); done
cargo build --locked --release -p lockdown-cli --bin lockdown "${args[@]}"

slices=()
for t in "${targets[@]}"; do slices+=("target/$t/release/lockdown"); done
rm -f "$out/lockdown"
lipo -create -output "$out/lockdown" "${slices[@]}"
chmod 0755 "$out/lockdown"
lipo -info "$out/lockdown"
