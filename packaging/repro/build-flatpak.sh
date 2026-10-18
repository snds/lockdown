#!/usr/bin/env bash
# Builds the Flatpak from the release binaries build-desktop.sh made, the
# way reproducible.yml compares it (docs/plan/29-release-polish.md §1). Runs
# in the digest-pinned flatpak-builder image (GNOME 47 SDK), privileged.
#
#   packaging/repro/build-flatpak.sh <stage dir> <out dir>
#
# Writes <out>/lockdown.flatpak and <out>/flatpak-hashes.txt with three
# lines: the bundle's SHA-256, the OSTree commit, and a SHA-256 over every
# file's path, mode and content checksum in the commit (`ostree ls -R -C`).
# OSTree stores no file times, so the content line depends only on the
# files. The commit carries a time and the builder's metadata; it is
# re-made with SOURCE_DATE_EPOCH as its time (`build-commit-from
# --timestamp`), so whether the commit and the bundle also match is what
# the comparison then shows.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
stage="$(cd "${1:?usage: build-flatpak.sh <stage dir> <out dir>}" && pwd)"
out="$(mkdir -p "${2:?usage: build-flatpak.sh <stage dir> <out dir>}" && cd "$2" && pwd)"
epoch="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}"
when="$(date -u -d "@$epoch" +%Y-%m-%dT%H:%M:%SZ)"
app=design.snds.lockdown
branch=stable
ref="app/$app/x86_64/$branch"
# Inside the checkout: flatpak-builder wants its state, build and repo on
# one file system.
work="$root/.flatpak-repro"
rm -rf "$work"
mkdir -p "$work"

command -v ostree >/dev/null 2>&1 || dnf install -y -q ostree >/dev/null
rm -rf "$root/packaging/flatpak/stage"
cp -r "$stage" "$root/packaging/flatpak/stage"
chmod +x "$root/packaging/flatpak/stage/lockdown-"*
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak-builder --user --disable-rofiles-fuse --force-clean --install-deps-from=flathub \
  --state-dir="$work/state" \
  --default-branch="$branch" --repo="$work/repo" "$work/build" "$root/packaging/flatpak/$app.yml"
ostree --repo="$work/repro" init --mode=archive
flatpak build-commit-from --src-repo="$work/repo" --timestamp="$when" --no-update-summary "$work/repro" "$ref"
flatpak build-bundle "$work/repro" "$out/lockdown.flatpak" "$app" "$branch"

commit="$(ostree --repo="$work/repro" rev-parse "$ref")"
content="$(ostree --repo="$work/repro" ls -R -C "$ref" | sha256sum | cut -d' ' -f1)"
{
  echo "$(sha256sum "$out/lockdown.flatpak" | cut -d' ' -f1)  flatpak:bundle"
  echo "$commit  flatpak:commit"
  echo "$content  flatpak:content"
} >"$out/flatpak-hashes.txt"
ostree --repo="$work/repro" show "$ref"
ostree --repo="$work/repro" ls -R -C "$ref" >"$out/flatpak-files.txt"
cat "$out/flatpak-hashes.txt"
