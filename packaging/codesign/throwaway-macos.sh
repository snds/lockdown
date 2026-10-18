#!/bin/sh
# Signs the code-signature test's binaries with throwaway certificates
# (docs/plan/29-release-polish.md §2), for ci.yml's `codesign-macos` job.
# Two self-signed code-signing certificates are made here, in a keychain of
# their own that lives as long as the job; neither is a real identity, and
# nothing Apple issued is involved. Then:
#
#   app-test        the test binary (plays the app), signed with A as design.snds.lockdown
#   peer-same       the peer (plays the host), signed with A as lockdown-native-host
#   peer-other      the peer, signed with B as lockdown-native-host
#   peer-unsigned   the peer as the linker left it (ad hoc on Apple silicon)
#   peer-wrong-id   the peer, signed with A under another identifier
#
#   packaging/codesign/throwaway-macos.sh <out dir> <test binary> <peer binary>
#
# Prints the environment the test reads, as KEY=value lines.
set -eu

out=$(mkdir -p "$1" && cd "$1" && pwd)
test_bin=$2
peer_bin=$3
openssl=/usr/bin/openssl
pass=$($openssl rand -hex 16)
kc="$out/throwaway.keychain-db"

security create-keychain -p "$pass" "$kc" >&2
security set-keychain-settings -lut 3600 "$kc" >&2
security unlock-keychain -p "$pass" "$kc" >&2
for who in A B; do
  cat >"$out/$who.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = Lockdown throwaway test signer $who
O = Lockdown CI
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF
  $openssl req -x509 -newkey rsa:2048 -nodes -days 1 -config "$out/$who.cnf" \
    -keyout "$out/$who.key" -out "$out/$who.crt" 2>/dev/null
  $openssl pkcs12 -export -inkey "$out/$who.key" -in "$out/$who.crt" \
    -out "$out/$who.p12" -passout "pass:$pass"
  security import "$out/$who.p12" -k "$kc" -P "$pass" -T /usr/bin/codesign >&2
  rm "$out/$who.key" "$out/$who.p12"
done
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pass" "$kc" >/dev/null
# shellcheck disable=SC2046
security list-keychains -d user -s "$kc" $(security list-keychains -d user | tr -d '"')

hash() { $openssl x509 -noout -fingerprint -sha1 -in "$out/$1.crt" | sed 's/.*=//; s/://g'; }
a=$(hash A)
b=$(hash B)
sign() { # <source> <dest> <certificate hash> <identifier>
  cp "$1" "$2"
  codesign --force --sign "$3" --keychain "$kc" --identifier "$4" "$2" >&2
  codesign --display --verbose=2 "$2" 2>&1 | sed 's/^/  /' >&2
}
sign "$test_bin" "$out/app-test" "$a" design.snds.lockdown
sign "$peer_bin" "$out/peer-same" "$a" lockdown-native-host
sign "$peer_bin" "$out/peer-other" "$b" lockdown-native-host
sign "$peer_bin" "$out/peer-wrong-id" "$a" lockdown-something-else
cp "$peer_bin" "$out/peer-unsigned"

echo "LOCKDOWN_CODESIGN_APP_TEST=$out/app-test"
echo "LOCKDOWN_CODESIGN_PEER_SAME=$out/peer-same"
echo "LOCKDOWN_CODESIGN_PEER_OTHER=$out/peer-other"
echo "LOCKDOWN_CODESIGN_PEER_UNSIGNED=$out/peer-unsigned"
echo "LOCKDOWN_CODESIGN_PEER_WRONG_ID=$out/peer-wrong-id"
