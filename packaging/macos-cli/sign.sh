#!/usr/bin/env bash
# Signs the universal `lockdown` command the way the app is signed
# (docs/plan/46-mac-cli-download.md §2):
#
#   - With a Developer ID (APPLE_SIGNING_IDENTITY names one in the
#     keychain): the hardened runtime and a secure timestamp. Then, when
#     APPLE_ID, APPLE_PASSWORD and APPLE_TEAM_ID are set too, notarised: a
#     zip holding the binary goes to Apple's notary service and must come
#     back Accepted. A bare binary can't be stapled, so macOS looks the
#     ticket up online the first time the command runs.
#   - Without one (APPLE_SIGNING_IDENTITY unset or "-"): ad hoc, as D450
#     seals the app. It says the file is intact, not who made it.
#
# Either way the signature names the identifier design.snds.lockdown.cli
# and carries no entitlements, so the command keeps its files in
# Application Support (docs/plan/41-mac-paths.md, D492).
#
#   packaging/macos-cli/sign.sh <binary>
set -euo pipefail

bin="${1:?usage: sign.sh <binary>}"
identity="${APPLE_SIGNING_IDENTITY:--}"
identifier="design.snds.lockdown.cli"

if [ "$identity" = "-" ]; then
  echo "signing $bin ad hoc (no Developer ID)"
  codesign --force --options runtime --identifier "$identifier" --sign - "$bin"
else
  echo "signing $bin with the Developer ID"
  codesign --force --options runtime --timestamp --identifier "$identifier" --sign "$identity" "$bin"
fi
codesign -dv "$bin" 2>&1

if [ "$identity" = "-" ]; then
  exit 0
fi
if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_PASSWORD:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; then
  echo "no notarisation account: signed with the Developer ID, not notarised"
  exit 0
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
zip="$work/lockdown-notarise.zip"
ditto -c -k --keepParent "$bin" "$zip"
echo "submitting to the notary service"
xcrun notarytool submit "$zip" \
  --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" \
  --wait --timeout 30m --output-format json > "$work/result.json"
cat "$work/result.json"
echo
status="$(plutil -extract status raw -o - "$work/result.json")"
id="$(plutil -extract id raw -o - "$work/result.json")"
xcrun notarytool log "$id" \
  --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" || true
if [ "$status" != "Accepted" ]; then
  echo "::error::the notary service said $status for the command line (submission $id)"
  exit 1
fi
echo "notarised (submission $id); a bare binary can't be stapled, so macOS checks the ticket online at first run"
