#!/bin/sh
# Checks a Mac build's code signature is the kind it should be
# (docs/plan/60-auto-update.md §3), for release.yml's `macos` job and CI's
# `keychain-macos` job:
#
#   packaging/macos/check-signature.sh developer-id|self-signed|adhoc <app or binary>
#
#   developer-id  signed with a team (Apple-issued); verifies strictly
#   self-signed   signed by a certificate with no team (the stable
#                 self-signed identity, D684); verifies strictly
#   adhoc         an ad-hoc signature (D450)
#
# For a .app it also requires the bundle to be sealed (resources sealed and
# Info.plist bound), whatever the kind.
set -eu

state=${1:?usage: check-signature.sh developer-id|self-signed|adhoc <path>}
path=${2:?usage: check-signature.sh developer-id|self-signed|adhoc <path>}
fail() { echo "::error::$path: $*"; exit 1; }

codesign --verify --deep --strict --verbose=2 "$path"
info=$(codesign -dvvv "$path" 2>&1)
echo "$info" | grep -E '^(Identifier|Authority|TeamIdentifier|Signature|CDHash)=' | sed 's/^/  /'
case "$path" in
  *.app | *.app/)
    case "$info" in *"Sealed Resources=none"* | *"Info.plist=not bound"*) fail "the bundle is not sealed" ;; esac
    ;;
esac
case "$state" in
  adhoc)
    case "$info" in *"Signature=adhoc"*) ;; *) fail "expected an ad-hoc signature" ;; esac
    ;;
  self-signed)
    case "$info" in *"Signature=adhoc"*) fail "expected a certificate's signature, found ad hoc" ;; esac
    echo "$info" | grep -q '^Authority=' || fail "no signing certificate"
    echo "$info" | grep -q '^TeamIdentifier=not set' || fail "a team is named: that is a Developer ID, not the self-signed identity"
    ;;
  developer-id)
    case "$info" in *"Signature=adhoc"*) fail "expected a Developer ID, found ad hoc" ;; esac
    team=$(echo "$info" | sed -n 's/^TeamIdentifier=//p')
    [ -n "$team" ] && [ "$team" != "not set" ] || fail "no team: not a Developer ID"
    ;;
  *) echo "check-signature.sh: the state is developer-id, self-signed or adhoc" >&2; exit 2 ;;
esac
echo "$path: $state signature, as expected"
