#!/usr/bin/env bash
# Checks a signed universal `lockdown` command on a Mac
# (docs/plan/46-mac-cli-download.md §3):
#
#   1. Both slices: lipo names arm64 and x86_64.
#   2. The signature holds (`codesign --verify --strict`), names
#      design.snds.lockdown.cli, has the hardened runtime and no
#      entitlements, and is the kind expected: a Developer ID with a secure
#      timestamp, or ad hoc and not merely the linker's.
#   3. It runs: `--version` names the version, a sealed `init` in a fresh
#      `--root` with the master password on stdin, then `list` with it,
#      and a wrong master password is refused. Where Rosetta is installed
#      the Intel slice runs too and opens the same vault.
#
#   packaging/macos-cli/check.sh adhoc|developer-id <binary> <version>
set -euo pipefail

kind="${1:?usage: check.sh adhoc|developer-id <binary> <version>}"
bin="${2:?usage: check.sh adhoc|developer-id <binary> <version>}"
version="${3:?usage: check.sh adhoc|developer-id <binary> <version>}"
fail() { echo "::error::$*" >&2; exit 1; }

echo "== 1. both architectures"
archs="$(lipo -archs "$bin")"
echo "$bin: $archs"
case " $archs " in *" arm64 "*) ;; *) fail "no arm64 slice in $bin" ;; esac
case " $archs " in *" x86_64 "*) ;; *) fail "no x86_64 slice in $bin" ;; esac

echo "== 2. the signature"
codesign --verify --strict --verbose=2 "$bin"
info="$(codesign -dv --verbose=4 "$bin" 2>&1)"
echo "$info"
grep -q '^Identifier=design.snds.lockdown.cli$' <<<"$info" || fail "the identifier isn't design.snds.lockdown.cli"
grep -Eq '^CodeDirectory .*flags=0x[0-9a-f]+\([^)]*runtime' <<<"$info" || fail "no hardened runtime"
entitlements="$(codesign -d --entitlements - "$bin" 2>/dev/null || true)"
if grep -q "<key>" <<<"$entitlements"; then
  echo "$entitlements"
  fail "the command line carries entitlements; it must carry none (D492)"
fi
case "$kind" in
  adhoc)
    grep -q '^Signature=adhoc$' <<<"$info" || fail "expected an ad-hoc signature"
    if grep -q 'linker-signed' <<<"$info"; then
      fail "only the linker's signature: sign.sh didn't sign it"
    fi
    ;;
  developer-id)
    grep -q '^Authority=Developer ID Application: ' <<<"$info" || fail "not signed with a Developer ID Application certificate"
    grep -q '^Timestamp=' <<<"$info" || fail "no secure timestamp"
    if grep -q '^TeamIdentifier=not set$' <<<"$info"; then
      fail "no team in the signature"
    fi
    ;;
  *)
    fail "the kind is adhoc or developer-id"
    ;;
esac
echo "ok: the $kind signature holds, with the hardened runtime and no entitlements"

echo "== 3. it runs"
out="$("$bin" --version)"
echo "$out"
[ "$out" = "lockdown $version" ] || fail "--version said \"$out\", not \"lockdown $version\""

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
master="correct horse battery staple orbit"
# Neither the runner's settings nor a test profile may steer it.
unset LOCKDOWN_ROOT LOCKDOWN_DATA LOCKDOWN_PASSWORD LOCKDOWN_MASTER_PASSWORD
root="$work/identity"
printf '%s\n' "$master" | "$bin" --root "$root" --master-password-stdin init --name Runner > "$work/kit.txt"
grep -q '^vault created for Runner; it opens only with your master password' "$work/kit.txt" || fail "init didn't say it made a sealed vault"
[ -f "$root/FORMAT" ] || fail "init made no identity at $root"
[ -f "$work/lockdown/device-key.sealed.cbor" ] || fail "init didn't seal the device key beside the root"
[ ! -e "$work/lockdown/device-key.cbor" ] || fail "init left a plain device key"
printf '%s\n' "$master" | "$bin" --root "$root" --master-password-stdin list
if printf '%s\n' "not the password" | "$bin" --root "$root" --master-password-stdin list > /dev/null 2>&1; then
  fail "a wrong master password opened the vault"
fi
echo "ok: a sealed init, list with the master password, and a wrong one refused"

if arch -x86_64 /usr/bin/true 2> /dev/null; then
  out="$(arch -x86_64 "$bin" --version)"
  [ "$out" = "lockdown $version" ] || fail "the Intel slice's --version said \"$out\""
  printf '%s\n' "$master" | arch -x86_64 "$bin" --root "$root" --master-password-stdin list
  echo "ok: the Intel slice runs (Rosetta) and opens the same vault"
else
  echo "no Rosetta on this Mac: the Intel slice was checked by lipo and codesign only"
fi
