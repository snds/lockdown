#!/bin/sh
# The installed-package smoke test (docs/plan/24-release.md §2): run inside
# a clean container after `apt-get install ./lockdown_*.deb` or
# `dnf install ./lockdown-*.rpm`, or against an unpacked tree with
# LOCKDOWN_PREFIX=<dir>. POSIX sh, so it runs in minimal images.
#
# It checks that the package put both programs where the app looks for the
# browser host (beside its own executable, D155), that every shared library
# they need resolved, that the desktop entry is there, and that the host
# runs from the package: no arguments → a framed "refused", a known
# extension with no app running → a framed "no_app".
set -eu

prefix="${LOCKDOWN_PREFIX:-}"
bin="$prefix/usr/bin"
fail() { echo "smoke: FAIL: $*" >&2; exit 1; }

for p in lockdown-desktop lockdown-native-host; do
  [ -x "$bin/$p" ] || fail "$bin/$p is missing or not executable"
done
echo "smoke: both programs installed in $bin"

if command -v ldd >/dev/null 2>&1; then
  for p in lockdown-desktop lockdown-native-host; do
    missing=$(ldd "$bin/$p" | grep "not found" || true)
    [ -z "$missing" ] || fail "$p needs libraries the package didn't bring: $missing"
  done
  echo "smoke: every shared library resolves"
fi

desktop=$(ls "$prefix"/usr/share/applications/*.desktop 2>/dev/null | head -n 1)
[ -n "$desktop" ] || fail "no desktop entry"
grep -q '^Exec=lockdown-desktop' "$desktop" || fail "$desktop doesn't start lockdown-desktop"
echo "smoke: desktop entry $desktop"

# A framed message is a 4-byte length then JSON; skip the length.
answer() { "$@" </dev/null 2>/dev/null | tail -c +5 || true; }

out=$(answer "$bin/lockdown-native-host")
echo "$out" | grep -q '"event":"refused"' || fail "the host without a caller said: $out"

empty=$(mktemp -d)
out=$(LOCKDOWN_DATA="$empty" answer "$bin/lockdown-native-host" chrome-extension://dbdblnolpiehhlabpmhobmkbjipmebpe/)
echo "$out" | grep -q '"event":"no_app"' || fail "the host with no app running said: $out"
rmdir "$empty"
echo "smoke: the host answers refused and no_app from the package"
echo "smoke: ok"
