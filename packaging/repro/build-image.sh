#!/usr/bin/env bash
# Builds the server image as an OCI archive, reproducibly
# (docs/plan/24-release.md §5): BuildKit stamps every layer and the image
# config with SOURCE_DATE_EPOCH (the commit's time unless set) and rewrites
# file times to it; provenance and SBOM attestations are left off because
# they record the build's own time and host. Nothing is pushed or loaded.
#
#   packaging/repro/build-image.sh <out dir> [<tag>]
#
# Writes <out>/lockdown-server-image.oci.tar and adds it to <out>/SHA256SUMS.
# Build apps/web first (pnpm build:wasm && pnpm build) to carry the web vault.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$(mkdir -p "${1:?usage: build-image.sh <out dir> [tag]}" && cd "$1" && pwd)"
tag="${2:-lockdown-server:repro}"
epoch="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}"

docker buildx build "$root" \
  --platform linux/amd64 \
  --build-arg SOURCE_DATE_EPOCH="$epoch" \
  --provenance=false --sbom=false \
  --tag "$tag" \
  --output "type=oci,dest=$out/lockdown-server-image.oci.tar,rewrite-timestamp=true"
(cd "$out" && sha256sum lockdown-server-image.oci.tar >> SHA256SUMS && tail -n 1 SHA256SUMS)
