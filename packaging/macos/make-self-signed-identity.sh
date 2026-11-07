#!/bin/sh
# Makes the stable self-signed code-signing identity the main channel's Mac
# builds are signed with when there is no Developer ID
# (docs/plan/60-auto-update.md §3, D684). Run it once, on your Mac:
#
#   sh packaging/macos/make-self-signed-identity.sh [folder]
#
# The folder defaults to ~/lockdown-signing. The script asks twice for a
# password for the .p12 file it writes (at least 12 characters; a script
# can give it as LOCKDOWN_IDENTITY_PASSWORD instead). It uses only what
# ships with macOS: /usr/bin/openssl (LibreSSL) and /usr/bin/security.
#
# What it makes, in that folder (readable only by you):
#   lockdown-self-signed.p12      the certificate and its private key, under the password
#   lockdown-self-signed.p12.b64  the same, base64, for the GitHub secret
#   lockdown-self-signed.crt      the certificate alone (public)
#
# Then it prints the two GitHub secrets to add. Keep the folder backed up
# and out of every repository: the keychain trusts whatever this certificate
# signs. A new certificate later means one more sign-in after the switch.
#
# Why: macOS's keychain gives the session token only to builds that satisfy
# the requirement of the build that saved it. An ad-hoc build's requirement
# is its own code, which every update changes; a build signed with this
# certificate is named by the certificate, which every build shares. It does
# nothing for Gatekeeper: only a Developer ID does that (D493).
set -eu

out=${1:-"$HOME/lockdown-signing"}
name="Lockdown self-signed (snds)"
openssl=/usr/bin/openssl
security=/usr/bin/security
[ -x "$openssl" ] || openssl=$(command -v openssl)

umask 077
mkdir -p "$out"
p12="$out/lockdown-self-signed.p12"
if [ -e "$p12" ]; then
  echo "make-self-signed-identity: $p12 already exists; move it away first (a new certificate means one more sign-in)" >&2
  exit 1
fi

if [ -n "${LOCKDOWN_IDENTITY_PASSWORD:-}" ]; then
  pass=$LOCKDOWN_IDENTITY_PASSWORD
else
  printf 'A password for the .p12 file (at least 12 characters): ' >&2
  stty -echo; read -r pass; stty echo; echo >&2
  printf 'The same password again: ' >&2
  stty -echo; read -r again; stty echo; echo >&2
  [ "$pass" = "$again" ] || { echo "make-self-signed-identity: the two passwords differ" >&2; exit 1; }
fi
[ "${#pass}" -ge 12 ] || { echo "make-self-signed-identity: the password needs at least 12 characters" >&2; exit 1; }
export LOCKDOWN_IDENTITY_PASSWORD="$pass"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cat >"$work/req.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $name
O = Lockdown
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
"$openssl" req -x509 -newkey rsa:3072 -nodes -days 3650 -config "$work/req.cnf" \
  -keyout "$work/key.pem" -out "$out/lockdown-self-signed.crt"
# 3DES and SHA-1 MAC: the .p12 form macOS's `security import` reads.
"$openssl" pkcs12 -export -inkey "$work/key.pem" -in "$out/lockdown-self-signed.crt" \
  -name "$name" -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 \
  -passout env:LOCKDOWN_IDENTITY_PASSWORD -out "$p12"
rm -f "$work/key.pem"

# Prove the file opens with the password and signs: a keychain of its own,
# a scratch binary signed with the identity, its signature read back.
kc="$work/check.keychain-db"
kcpass=$("$openssl" rand -hex 16)
old_list=$("$security" list-keychains -d user | tr -d '"')
"$security" create-keychain -p "$kcpass" "$kc" >/dev/null
# codesign finds an identity only in a keychain on the search list; this
# one is on it for the check and taken off again straight after.
cleanup() {
  # shellcheck disable=SC2086
  "$security" list-keychains -d user -s $old_list 2>/dev/null || true
  "$security" delete-keychain "$kc" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
"$security" unlock-keychain -p "$kcpass" "$kc"
"$security" import "$p12" -k "$kc" -P "$pass" -T /usr/bin/codesign >/dev/null
"$security" set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$kcpass" "$kc" >/dev/null
# shellcheck disable=SC2086
"$security" list-keychains -d user -s "$kc" $old_list
sha1=$("$openssl" x509 -noout -fingerprint -sha1 -in "$out/lockdown-self-signed.crt" | sed 's/.*=//; s/://g')
# The certificate and its private key both came in (an identity, not a lone certificate).
"$security" find-identity -p codesigning "$kc" | grep -q "$sha1" \
  || { echo "make-self-signed-identity: the .p12 imported no signing identity (the private key is missing)" >&2; exit 1; }
printf 'lockdown signing check\n' >"$work/probe"
codesign --force --sign "$sha1" --keychain "$kc" --identifier design.snds.lockdown "$work/probe"
codesign -dvvv "$work/probe" 2>&1 | grep -q "Authority=$name" || { echo "make-self-signed-identity: the identity didn't sign" >&2; exit 1; }
cleanup
trap - EXIT

base64 <"$p12" | tr -d '\n' >"$p12.b64"
chmod 600 "$p12" "$p12.b64"

cat <<DONE

Made the signing identity "$name"
  certificate SHA-1: $sha1
  valid for 10 years, in $out

Add two secrets to the repository (Settings > Secrets and variables > Actions):

  MACOS_SELFSIGNED_CERTIFICATE           the contents of $p12.b64
  MACOS_SELFSIGNED_CERTIFICATE_PASSWORD  the password you just chose

or with the GitHub CLI:

  gh secret set MACOS_SELFSIGNED_CERTIFICATE --repo snds/lockdown < "$p12.b64"
  gh secret set MACOS_SELFSIGNED_CERTIFICATE_PASSWORD --repo snds/lockdown

(the second one asks for the password you just chose).

Keep $out backed up and out of every repository. Builds from main are
signed with it from the next merge; the first such build asks you to sign in
once with your master password, and none after it.
DONE
