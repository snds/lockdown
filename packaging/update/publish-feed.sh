#!/usr/bin/env bash
# Publishes a feed to a rolling pre-release on the shared releases
# repository (docs/plan/66-shared-feed.md): `lockdown-main` and
# `lockdown-server-main` on snds/releases. release.yml runs it; nothing else
# does. It talks to GitHub only through `gh`, which reads the token from
# GH_TOKEN, so the token is never an argument and never printed.
#
#   publish-feed.sh previous <repo> <tag> <out>
#       The published latest.json, written to <out>. Nothing is written when
#       the release, or its latest.json, isn't there yet. It fails when one
#       is there and can't be read: a new manifest is never written over one
#       that couldn't be compared.
#
#   publish-feed.sh publish <repo> <tag> <title> <notes> <dir>
#       Replaces the release's files with <dir>'s. <dir> must hold
#       latest.json. Every other file goes up first and latest.json last, so
#       the old manifest names the old files until the new one replaces it.
#       Then files <dir> doesn't hold are removed. The release is always a
#       pre-release and never "latest".
#
# The release is never deleted and its tag is never moved: the repository
# is not this one, so there is no commit of ours to point at.
set -euo pipefail

usage() {
  sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

# The token must reach the repository, and a release needs a commit to tag.
reach() {
  gh api "repos/$1/commits?per_page=1" > /dev/null 2>&1 || {
    echo "::error::the releases token can't read $1 (expired, or not scoped to it), or $1 has no commits yet"
    exit 1
  }
}

# Whether the release is there. Only GitHub's own "release not found" means
# no; any other failure (the network, a rate limit, a server error) stops
# here, so a missing answer is never taken for an empty feed and the
# never-backwards check is never skipped.
exists() {
  local err
  if err=$(gh release view "$2" --repo "$1" 2>&1 > /dev/null); then
    return 0
  fi
  case "$err" in
    *"release not found"*) return 1 ;;
  esac
  echo "::error::couldn't tell whether $2 is on $1: $err"
  exit 1
}

names() {
  gh release view "$2" --repo "$1" --json assets -q '.assets[].name'
}

[ $# -ge 1 ] || usage
command="$1"
shift

case "$command" in
  previous)
    [ $# -eq 3 ] || usage
    repo="$1" tag="$2" out="$3"
    reach "$repo"
    rm -f "$out"
    if ! exists "$repo" "$tag"; then
      echo "$tag isn't on $repo yet: this run starts it"
      exit 0
    fi
    listed=$(names "$repo" "$tag")
    if ! printf '%s\n' "$listed" | grep -qxF latest.json; then
      echo "$tag on $repo has no latest.json yet"
      exit 0
    fi
    gh release download "$tag" --repo "$repo" --pattern latest.json --output "$out"
    [ -s "$out" ] || { echo "::error::$tag's latest.json on $repo came back empty"; exit 1; }
    ;;
  publish)
    [ $# -eq 5 ] || usage
    repo="$1" tag="$2" title="$3" notes="$4" dir="$5"
    [ -s "$dir/latest.json" ] || { echo "::error::$dir has no latest.json"; exit 1; }
    reach "$repo"
    if exists "$repo" "$tag"; then
      gh release edit "$tag" --repo "$repo" --prerelease --latest=false --title "$title" --notes "$notes"
    else
      gh release create "$tag" --repo "$repo" --prerelease --latest=false --title "$title" --notes "$notes"
    fi
    keep=$(cd "$dir" && ls)
    files=()
    for f in "$dir"/*; do
      [ "$(basename "$f")" = latest.json ] || files+=("$f")
    done
    if [ ${#files[@]} -gt 0 ]; then
      gh release upload "$tag" --repo "$repo" --clobber "${files[@]}"
    fi
    gh release upload "$tag" --repo "$repo" --clobber "$dir/latest.json"
    names "$repo" "$tag" | while read -r old; do
      printf '%s\n' "$keep" | grep -qxF "$old" || gh release delete-asset "$tag" "$old" --repo "$repo" --yes
    done
    gh release view "$tag" --repo "$repo" --json isPrerelease,tagName,assets \
      -q '{prerelease: .isPrerelease, tag: .tagName, files: [.assets[].name]}'
    ;;
  *)
    usage
    ;;
esac
