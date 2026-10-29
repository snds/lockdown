#!/usr/bin/env bash
# Whether the developer tools (docs/plan/40-devtools.md) are in a build.
#
#   packaging/devtools/check.sh absent  <file or folder>...   a tagged release: fails if any is found
#   packaging/devtools/check.sh present <file or folder>...   a dry run: fails if none is, proving the check sees them
#
# It looks for strings only a build with the tools carries: the marker
# (lockdown_devtools::MARKER, apps/desktop/src/devtools/marker.ts), the
# plugin's command names, and the default demo pack's name and format. A
# folder is searched whole (the front end before Tauri compresses it into
# the binary); a file as bytes (`grep -a`), so a binary is searched too.
set -euo pipefail

mode="${1:?usage: check.sh absent|present <file or folder>...}"
shift
[ "$#" -gt 0 ] || { echo "check.sh: name a file or folder" >&2; exit 2; }

needles=(
  "lockdown-devtools-6f1c2a"
  "plugin:devtools|"
  "demo_load_pack"
  "reset_plan"
  "lockdown-demo-pack"
  "Lockdown QA pack"
)

found=0
for target in "$@"; do
  [ -e "$target" ] || { echo "check.sh: $target doesn't exist" >&2; exit 2; }
  for n in "${needles[@]}"; do
    if grep -a -r -F -l -q -- "$n" "$target"; then
      echo "$target: has \"$n\""
      found=$((found + 1))
    fi
  done
done

case "$mode" in
  absent)
    if [ "$found" -gt 0 ]; then
      echo "::error::the developer tools are in a release build ($found hits); a tag build must build with LOCKDOWN_DEVTOOLS=off and src-tauri/tauri.release.conf.json"
      exit 1
    fi
    echo "no developer tools in: $*"
    ;;
  present)
    if [ "$found" -eq 0 ]; then
      echo "::error::no sign of the developer tools in a dry run's build, so the release check would see nothing either"
      exit 1
    fi
    echo "the developer tools are in this dry run's build ($found hits), as they should be"
    ;;
  *)
    echo "check.sh: the mode is absent or present" >&2
    exit 2
    ;;
esac
