#!/bin/sh
# What macOS does with the session token's keychain item across two builds
# whose code identities differ, as two updates in a row would
# (docs/plan/60-auto-update.md §3). For ci.yml's `keychain-macos` job.
#
#   packaging/update/keychain-check.sh <out dir> <probe built as A> <probe built as B>
#
# The probes are the same program built twice (LOCKDOWN_PROBE_BUILD=a and
# =b), so their bytes and code hashes differ. Everything happens in a
# keychain made here and set as the default for the job; the runner's own
# keychains are put back at the end. Every read is made with the keychain's
# dialogs switched off, so a read macOS would have asked about shows as
# "refused <code>" instead of hanging the job.
#
# What it proves, in order:
#   1. Ad hoc (D450): build A writes the item; A reads it back; B, a
#      different ad-hoc build, is refused: a person would see the dialog.
#   2. The fix: both ad-hoc builds run in read-only mode, and B's read
#      through the app's own store says "absent" (sign in again), with no
#      dialog and no hang.
#   3. A stable identity (one throwaway self-signed certificate, both
#      builds): B reads what A wrote, without a dialog. This is the
#      Developer ID path (D493) in miniature: the access list names the
#      certificate, which every build shares.
set -eu

out=$(mkdir -p "$1" && cd "$1" && pwd)
probe_a=$2
probe_b=$3
openssl=/usr/bin/openssl
pass=$($openssl rand -hex 16)
kc="$out/probe.keychain-db"
old_default=$(security default-keychain -d user | tr -d ' "')
old_list=$(security list-keychains -d user | tr -d '"')

restore() {
  security default-keychain -d user -s "$old_default" || true
  # shellcheck disable=SC2086
  security list-keychains -d user -s $old_list || true
  security delete-keychain "$kc" 2>/dev/null || true
}
trap restore EXIT

security create-keychain -p "$pass" "$kc"
security set-keychain-settings -lut 3600 "$kc"
security unlock-keychain -p "$pass" "$kc"
# shellcheck disable=SC2086
security list-keychains -d user -s "$kc" $old_list
security default-keychain -d user -s "$kc"

fail() { echo "::error::$*"; exit 1; }
expect() { # <what> <expected> <actual>
  echo "  $1: $3"
  case "$3" in $2) ;; *) fail "$1: expected $2, got $3" ;; esac
}

cp "$probe_a" "$out/adhoc-a"
cp "$probe_b" "$out/adhoc-b"
codesign --force --sign - --identifier design.snds.lockdown "$out/adhoc-a"
codesign --force --sign - --identifier design.snds.lockdown "$out/adhoc-b"
ha=$(codesign -dv "$out/adhoc-a" 2>&1 | sed -n 's/^CDHash=//p')
hb=$(codesign -dv "$out/adhoc-b" 2>&1 | sed -n 's/^CDHash=//p')
echo "ad hoc: A $ha, B $hb"
[ "$ha" != "$hb" ] || fail "the two builds have the same code hash, so this proves nothing"

echo "1. an item an ad-hoc build wrote, read by the next ad-hoc build"
expect "A writes" written "$("$out/adhoc-a" write session:probe@adhoc token-a)"
expect "A reads, dialogs off" found "$("$out/adhoc-a" quiet-read session:probe@adhoc)"
expect "B reads, dialogs off" "refused *" "$("$out/adhoc-b" quiet-read session:probe@adhoc)"
echo "  so with dialogs on, B would have asked the person: the prompt this slice removes"

echo "2. what the app does now in an ad-hoc build"
expect "A's mode" read-only "$("$out/adhoc-a" mode)"
expect "B's mode" read-only "$("$out/adhoc-b" mode)"
expect "B's own read (sign in again)" absent "$("$out/adhoc-b" store-read session:probe@adhoc)"

echo "3. one stable identity across both builds"
cat >"$out/stable.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = Lockdown throwaway stable signer
O = Lockdown CI
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
$openssl req -x509 -newkey rsa:2048 -nodes -days 1 -config "$out/stable.cnf" \
  -keyout "$out/stable.key" -out "$out/stable.crt" 2>/dev/null
$openssl pkcs12 -export -inkey "$out/stable.key" -in "$out/stable.crt" \
  -out "$out/stable.p12" -passout "pass:$pass"
security import "$out/stable.p12" -k "$kc" -P "$pass" -T /usr/bin/codesign >/dev/null
rm "$out/stable.key" "$out/stable.p12"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pass" "$kc" >/dev/null
cert=$($openssl x509 -noout -fingerprint -sha1 -in "$out/stable.crt" | sed 's/.*=//; s/://g')
cp "$probe_a" "$out/stable-a"
cp "$probe_b" "$out/stable-b"
codesign --force --sign "$cert" --keychain "$kc" --identifier design.snds.lockdown "$out/stable-a"
codesign --force --sign "$cert" --keychain "$kc" --identifier design.snds.lockdown "$out/stable-b"
codesign -d -r- "$out/stable-b" 2>&1 | sed 's/^/  /'
expect "A writes" written "$("$out/stable-a" write session:probe@stable token-s)"
expect "B reads, dialogs off" found "$("$out/stable-b" quiet-read session:probe@stable)"
echo "keychain check passed"
