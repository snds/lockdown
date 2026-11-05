#!/usr/bin/env bash
# Whether a native build of lockdown-app has the web vault's plain
# device-link join (docs/plan/57-fence-web-join.md).
#
#   packaging/plain-join/check.sh
#
# `App::join_start` and `App::join_finish_signed_in` write and finish the
# plain key file only the web vault's tab seals, so they are compiled only
# for the web vault's target (`wasi`) and with lockdown-app's
# `test-support` feature, which only `[dev-dependencies]` turn on.
#
# This builds lockdown-app's library twice, natively, and reads the
# symbols each one defines:
#
# 1. With `test-support` (a dry run): the plain join must be there, so the
#    check is known to see it.
# 2. As a release resolves it (default features, no dev-dependencies):
#    the plain join must be missing, and the sealed join
#    (`join_start_sealed`) must be there, so the library was read.
#
# The fence is a `cfg` on the target and a feature, never on the profile,
# so the dev profile's library has exactly the items a release's has.
# The desktop, the server and the command line resolve lockdown-app
# without `test-support` (the `cargo tree` step beside this one in CI).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here/../.."

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Builds lockdown-app's library with these arguments and copies the rlib
# to `$work/<name>.rlib` (cargo puts every build's at the same path).
rlib() {
  local name="$1"
  shift
  local built
  built="$(cargo build -p lockdown-app --lib --message-format=json "$@" \
    | jq -r 'select(.reason == "compiler-artifact" and .target.name == "lockdown_app")
             | .filenames[] | select(endswith(".rlib"))' \
    | tail -n 1)"
  [ -n "$built" ] && [ -f "$built" ] || { echo "check.sh: cargo named no rlib for lockdown-app" >&2; exit 2; }
  cp "$built" "$work/$name.rlib"
  echo "$work/$name.rlib"
}

# The functions the library defines, demangled, one per line.
defined() {
  nm -C --defined-only "$1" 2>/dev/null | sed -n 's/^[0-9a-fA-F]* [TtWw] //p'
}

# Whether the library defines App's method `$2` (exactly that name), in
# either symbol mangling: `<lockdown_app::App>::name` (v0) or
# `lockdown_app::devices::<impl lockdown_app::App>::name` (legacy).
has() {
  grep -Eq "(<lockdown_app::App>|<impl lockdown_app::App>)::$2(::h[0-9a-f]{16})?\$" <<<"$1"
}

with="$(rlib with --features test-support)"
without="$(rlib without)"

syms_with="$(defined "$with")"
syms_without="$(defined "$without")"

fail=0
for name in join_start join_finish_signed_in; do
  if ! has "$syms_with" "$name"; then
    echo "check.sh: the dry run's library (test-support) has no App::$name, so the check sees nothing" >&2
    fail=1
  fi
  if has "$syms_without" "$name"; then
    echo "check.sh: a native build without test-support has App::$name, the plain device-link join" >&2
    fail=1
  fi
done
if ! has "$syms_without" join_start_sealed; then
  echo "check.sh: a native build without test-support has no App::join_start_sealed either, so its symbols weren't read" >&2
  fail=1
fi
[ "$fail" -eq 0 ] || exit 1
echo "a native build of lockdown-app has no plain device-link join (join_start, join_finish_signed_in); the sealed join is there"
