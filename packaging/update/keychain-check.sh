#!/bin/sh
# What the session token's keychain items do across updates on a Mac
# (docs/plan/60-auto-update.md §3, D684), for ci.yml's `keychain-macos` job.
#
#   packaging/update/keychain-check.sh <out dir> <probe built as A> <probe built as B>
#
# The probes are the same program built twice (LOCKDOWN_PROBE_BUILD=a and =b
# baked in at compile time), so their bytes and code hashes differ, as two
# merges' builds do. They go through the app's own keychain store. Everything
# happens in a keychain made here and set as the default for the job; the
# runner's own keychains are put back at the end. Every call runs with the
# keychain's dialogs switched off, so anything macOS would have asked about
# shows as a refusal instead of hanging the job, and the run fails if
# errSecInteractionNotAllowed (-25308) appears anywhere.
#
# 1. Ad hoc (D450), two builds:
#    - A saves its token and reads it back;
#    - B's read of A's item is refused, silently (B signs in again);
#    - B saves its own token, with A's item still there, and reads it back;
#    - an item from before identity tags (the bare key) is the same: refused
#      silently to B, and B's own save beside it works;
#    - whether B can delete A's stale item without a dialog (reported).
# 2. The stable self-signed identity, made by
#    packaging/macos/make-self-signed-identity.sh exactly as Sean runs it:
#    both builds signed with it; B reads A's token, with no sign-in.
# 3. packaging/macos/check-signature.sh accepts each build's state.
set -eu

out=$(mkdir -p "$1" && cd "$1" && pwd)
probe_a=$2
probe_b=$3
here=$(cd "$(dirname "$0")/../.." && pwd)
openssl=/usr/bin/openssl
pass=$($openssl rand -hex 16)
kc="$out/probe.keychain-db"
old_default=$(security default-keychain -d user | tr -d ' "')
old_list=$(security list-keychains -d user | tr -d '"')
log="$out/answers.txt"
: >"$log"

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
# Runs a probe, keeps its answer, and checks it against a pattern.
expect() { # <what> <pattern> <probe> <args>...
  what=$1 pattern=$2
  shift 2
  got=$("$@")
  echo "$what: $got" | tee -a "$log"
  case "$got" in $pattern) ;; *) fail "$what: expected $pattern, got $got" ;; esac
}
cdhash() { codesign -dvvv "$1" 2>&1 | sed -n 's/^CDHash=//p' | head -n 1; }

key="session:probe@https://server.example"
legacy="session:legacy@https://server.example"

cp "$probe_a" "$out/adhoc-a"
cp "$probe_b" "$out/adhoc-b"
codesign --force --sign - --identifier design.snds.lockdown "$out/adhoc-a"
codesign --force --sign - --identifier design.snds.lockdown "$out/adhoc-b"
ha=$(cdhash "$out/adhoc-a")
hb=$(cdhash "$out/adhoc-b")
echo "ad hoc: A $ha ($("$out/adhoc-a" build)), B $hb ($("$out/adhoc-b" build))"
[ -n "$ha" ] && [ -n "$hb" ] || fail "codesign named no code hash, so this proves nothing"
[ "$ha" != "$hb" ] || fail "the two builds have the same code hash, so this proves nothing"
ta=$("$out/adhoc-a" tag)
tb=$("$out/adhoc-b" tag)
echo "identity tags: A $ta, B $tb"
case "$ta" in build-*) ;; *) fail "an ad-hoc build should be tagged by its code, got $ta" ;; esac
[ "$ta" != "$tb" ] || fail "two ad-hoc builds share a tag"

echo "1. two ad-hoc builds, one after the other"
expect "A saves its token" written "$out/adhoc-a" store-write "$key" token-a
expect "A reads it back" "found token-a" "$out/adhoc-a" store-read "$key"
expect "B reads A's item directly, dialogs off" "refused *" "$out/adhoc-b" raw-read "$key#$ta"
expect "B's own read (sign in again, no dialog)" absent "$out/adhoc-b" store-read "$key"
expect "B saves its fresh token beside A's" written "$out/adhoc-b" store-write "$key" token-b
expect "B reads its own back" "found token-b" "$out/adhoc-b" store-read "$key"
expect "A still reads its own" "found token-a" "$out/adhoc-a" store-read "$key"
echo "   an item from before identity tags (the bare key)"
expect "an older build saves under the bare key" written "$out/adhoc-a" legacy-write "$legacy" token-old
expect "B's own read (sign in again, no dialog)" absent "$out/adhoc-b" store-read "$legacy"
expect "B saves its fresh token" written "$out/adhoc-b" store-write "$legacy" token-new
expect "B reads it back" "found token-new" "$out/adhoc-b" store-read "$legacy"
echo "   the items left behind"
"$out/adhoc-b" accounts | sed 's/^/  item: /' | tee -a "$log"
expect "B deletes A's stale item, dialogs off" "*" "$out/adhoc-b" raw-delete "$key#$ta"
expect "B deletes the bare-key item, dialogs off" "*" "$out/adhoc-b" raw-delete "$legacy"
expect "B deletes its own item, dialogs off" deleted "$out/adhoc-b" raw-delete "$key#$tb"

echo "2. the stable self-signed identity, made as Sean makes it"
idpass=$($openssl rand -hex 16)
LOCKDOWN_IDENTITY_PASSWORD="$idpass" sh "$here/packaging/macos/make-self-signed-identity.sh" "$out/identity" >"$out/identity.txt" \
  || { cat "$out/identity.txt"; fail "make-self-signed-identity.sh failed"; }
cat "$out/identity.txt"
for secret in MACOS_SELFSIGNED_CERTIFICATE MACOS_SELFSIGNED_CERTIFICATE_PASSWORD; do
  grep -q "^  $secret " "$out/identity.txt" || fail "the script didn't name the secret $secret in plain words"
  grep -q "gh secret set $secret --repo snds/lockdown" "$out/identity.txt" || fail "the script didn't give the command for $secret"
done
[ -s "$out/identity/lockdown-self-signed.p12.b64" ] || fail "no base64 .p12"
# Imported from the base64 secret's form, as release.yml's macos job does.
base64 -D -i "$out/identity/lockdown-self-signed.p12.b64" -o "$out/secret.p12"
security import "$out/secret.p12" -k "$kc" -P "$idpass" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pass" "$kc" >/dev/null
rm "$out/secret.p12"
cert=$(security find-identity -p codesigning "$kc" | sed -n 's/^ *[0-9]*) \([0-9A-F]\{40\}\) "Lockdown self-signed (snds)".*/\1/p' | head -n 1)
[ -n "$cert" ] || fail "the imported identity isn't listed for code signing"
echo "signing identity $cert"
cp "$probe_a" "$out/stable-a"
cp "$probe_b" "$out/stable-b"
codesign --force --sign "$cert" --keychain "$kc" --identifier design.snds.lockdown "$out/stable-a"
codesign --force --sign "$cert" --keychain "$kc" --identifier design.snds.lockdown "$out/stable-b"
codesign -d -r- "$out/stable-b" 2>&1 | sed 's/^/  /'
[ "$(cdhash "$out/stable-a")" != "$(cdhash "$out/stable-b")" ] || fail "the signed builds have the same code hash"
sa=$("$out/stable-a" tag)
sb=$("$out/stable-b" tag)
echo "identity tags: A $sa, B $sb"
case "$sa" in cert-*) ;; *) fail "a certificate-signed build should be tagged by its certificate, got $sa" ;; esac
[ "$sa" = "$sb" ] || fail "two builds signed by one certificate have different tags"
skey="session:stable@https://server.example"
expect "A saves its token" written "$out/stable-a" store-write "$skey" token-s
expect "B reads it, no sign-in" "found token-s" "$out/stable-b" store-read "$skey"
expect "B reads the item directly, dialogs off" found "$out/stable-b" raw-read "$skey#$sb"
expect "B renews it in place" written "$out/stable-b" store-write "$skey" token-s2
expect "A reads B's renewal" "found token-s2" "$out/stable-a" store-read "$skey"

echo "3. each build's signature state"
sh "$here/packaging/macos/check-signature.sh" adhoc "$out/adhoc-a"
sh "$here/packaging/macos/check-signature.sh" self-signed "$out/stable-a"
if sh "$here/packaging/macos/check-signature.sh" self-signed "$out/adhoc-b" >/dev/null 2>&1; then
  fail "check-signature.sh took an ad-hoc build for the self-signed identity"
fi
if sh "$here/packaging/macos/check-signature.sh" developer-id "$out/stable-b" >/dev/null 2>&1; then
  fail "check-signature.sh took the self-signed identity for a Developer ID"
fi

if grep -q -- "-25308" "$log"; then
  fail "the keychain wanted to ask the person (errSecInteractionNotAllowed) somewhere above"
fi
rm -rf "$out/identity"
echo "keychain check passed: no answer above needed a dialog"
