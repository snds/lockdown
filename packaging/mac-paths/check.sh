#!/usr/bin/env bash
# Where Lockdown keeps its files on a real Mac (docs/plan/41-mac-paths.md
# §3), with a fresh $HOME each time:
#
#   1. The app's own resolution (the `paths` example of lockdown-app) and
#      the command line both put the identity and the data under
#      Library/Application Support/snds/, and nothing under
#      Library/Group Containers.
#   2. An ad-hoc signature that names the app group still leads to
#      Application Support: only a Developer ID signature may.
#   3. A vault an older build left in Group Containers is moved once, and
#      opens at the new places with the same master password.
#
# Usage: check.sh <lockdown CLI> <paths example>
set -euo pipefail

cli="$1"
example="$2"
work="$(mktemp -d)"
master="correct horse battery staple orbit"
fail() { echo "FAIL: $*" >&2; exit 1; }
field() { grep "^$1=" | head -n 1 | cut -d= -f2-; }

fresh_home() {
  local h="$work/$1"
  mkdir -p "$h"
  echo "$h"
}

# Neither the runner's own settings nor a test profile may steer it.
unset LOCKDOWN_ROOT LOCKDOWN_DATA LOCKDOWN_PASSWORD LOCKDOWN_MASTER_PASSWORD XDG_DATA_HOME

echo "== 1. a fresh Mac"
HOME="$(fresh_home one)"
export HOME
out="$("$example")"
echo "$out"
support="$HOME/Library/Application Support/snds"
[ "$(echo "$out" | field store)" = "application-support" ] || fail "the store isn't Application Support"
[ "$(echo "$out" | field root)" = "$support/identity" ] || fail "the app's identity root"
[ "$(echo "$out" | field data)" = "$support/lockdown" ] || fail "the app's data directory"
[ "$(echo "$out" | field host_data)" = "$support/lockdown" ] || fail "the browser host's data directory"
[ -d "$support/lockdown" ] || fail "the probe didn't make the data directory"
printf '%s\n' "$master" | "$cli" init --name Runner --password-stdin > "$work/kit.txt"
printf '%s\n' "$master" | "$cli" --master-password-stdin list
[ -f "$support/identity/FORMAT" ] || fail "no identity at $support/identity"
[ -f "$support/lockdown/device-key.sealed.cbor" ] || fail "no sealed device key at $support/lockdown"
[ ! -e "$HOME/Library/Group Containers" ] || fail "something was made under Library/Group Containers"
echo "ok: the app, the browser host and the command line keep everything in $support"

echo "== 2. an ad-hoc signature naming the app group"
HOME="$(fresh_home two)"
export HOME
cat > "$work/groups.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.application-groups</key>
  <array><string>ABCDE12345.design.snds.identity</string></array>
</dict>
</plist>
PLIST
cp "$example" "$work/paths-adhoc"
codesign --force --sign - --entitlements "$work/groups.plist" "$work/paths-adhoc"
codesign -d --entitlements - "$work/paths-adhoc" 2>&1 | grep -q "ABCDE12345.design.snds.identity" \
  || fail "the ad-hoc copy doesn't carry the entitlement"
set +e
out="$("$work/paths-adhoc")"
status=$?
set -e
echo "$out"
if [ "$status" -ge 128 ]; then
  # macOS may refuse to run an ad-hoc program that claims a group at all,
  # which makes the same point (D450).
  echo "ok: macOS refused to run the ad-hoc copy (signal $((status - 128)))"
else
  [ "$status" -eq 0 ] || fail "the ad-hoc copy failed ($status)"
  [ "$(echo "$out" | field groups)" = "ABCDE12345.design.snds.identity" ] \
    || fail "the entitlement wasn't read from the signature"
  [ "$(echo "$out" | field signed)" = "false" ] || fail "an ad-hoc signature read as signed"
  [ "$(echo "$out" | field store)" = "application-support" ] || fail "an ad-hoc signature led to a group container"
  [ ! -e "$HOME/Library/Group Containers" ] || fail "something was made under Library/Group Containers"
  echo "ok: the group was read, and an ad-hoc signature still keeps everything in Application Support"
fi

echo "== 3. a vault an older build left in Group Containers"
HOME="$(fresh_home three)"
export HOME
old_root="$HOME/Library/Group Containers/group.design.snds.identity"
old_data="$HOME/Library/Group Containers/lockdown"
support="$HOME/Library/Application Support/snds"
printf '%s\n' "$master" | "$cli" --root "$old_root" init --name Runner --password-stdin > "$work/kit3.txt"
printf '%s\nitem-password\n' "$master" | "$cli" --root "$old_root" --master-password-stdin \
  add login --site https://moved.example --username runner --password-stdin
[ -f "$old_data/device-key.sealed.cbor" ] || fail "the older layout wasn't made"
printf '%s\n' "$master" | "$cli" --master-password-stdin list > "$work/list3.txt" 2> "$work/err3.txt"
cat "$work/err3.txt" "$work/list3.txt"
grep -q "moved.example" "$work/list3.txt" || fail "the moved vault didn't open"
grep -q "moved Lockdown's data" "$work/err3.txt" || fail "the move wasn't reported"
[ -f "$support/identity/FORMAT" ] || fail "the identity wasn't copied"
[ -f "$support/lockdown/device-key.sealed.cbor" ] || fail "the data wasn't moved"
[ ! -e "$old_data" ] || fail "the old data folder is still there"
[ -f "$support/lockdown-moved-from-group-containers.txt" ] || fail "no note of the move"
printf '%s\n' "$master" | "$cli" --master-password-stdin list 2> "$work/err3b.txt" > /dev/null
! grep -q "moved" "$work/err3b.txt" || fail "the move ran twice"
echo "ok: the older vault moved once and opens at $support"
