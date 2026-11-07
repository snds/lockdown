#!/usr/bin/env bash
# The end-to-end update (docs/plan/60-auto-update.md §7), for ci.yml's
# `update-e2e` job: install build A, serve build B from this computer
# signed with the job's throwaway key, let A find, download, check and
# install B and restart, then see that B runs and opens the vault A had.
#
#   packaging/update/e2e-run.sh <dir e2e-build.sh filled>
#
# 1. The CLI makes a throwaway vault with one item ("Before the update").
# 2. A latest.json naming B, with B's signature, is served on 127.0.0.1.
# 3. A is installed: the .app copied to a folder this account owns (macOS);
#    the NSIS installer run silently, per user (Windows).
# 4. A starts with the test-only endpoint and key, the vault's password and
#    the restart-when-ready hook; its report and then B's go to report.txt.
# 5. Passes when B reports it started and opened the vault with its one
#    item; the CLI then lists the item again from the same vault.
set -euo pipefail

out=$(cd "$1" && pwd)
A=0.1.0-main.20260101000000
B=0.1.0-main.20260102000000
port=8765
pw="update-e2e-throwaway-$(date +%s)"
py=$(command -v python3 || command -v python)
cli="$out/lockdown"
[ -f "$cli.exe" ] && cli="$cli.exe"
report="$out/report.txt"
: >"$report"

# The app reads LOCKDOWN_ROOT; the CLI takes the same place as --root. Both
# put the data directory beside it.
export LOCKDOWN_ROOT="$out/home/snds/identity"
mkdir -p "$LOCKDOWN_ROOT"
export LOCKDOWN_MASTER_PASSWORD="$pw"
lockdown() { "$cli" --root "$LOCKDOWN_ROOT" "$@"; }

echo "1. a vault"
lockdown init --name "Update e2e" >/dev/null
printf 'item-password-e2e\n' | lockdown add login --site https://example.org --username e2e --password-stdin --title "Before the update"
lockdown list
ls "$out/home/snds"

echo "2. B, served"
mkdir -p "$out/serve"
case "$(uname -s)" in
  Darwin) os=mac; bundle="Lockdown_${B}.app.tar.gz" ;;
  *) os=win; bundle="Lockdown_${B}_x64-setup.exe" ;;
esac
cp "$out/B/$bundle" "$out/B/$bundle.sig" "$out/serve/"
"$py" - "$out/serve" "$bundle" "$B" "$port" <<'PY'
import json, sys
from pathlib import Path
serve, bundle, version, port = Path(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
entry = {"url": f"http://127.0.0.1:{port}/{bundle}", "signature": (serve / f"{bundle}.sig").read_text().strip()}
keys = ["darwin-aarch64", "darwin-x86_64", "windows-x86_64", "windows-x86_64-nsis"]
(serve / "latest.json").write_text(json.dumps({"version": version, "notes": "", "pub_date": "2026-01-02T00:00:00Z", "platforms": {k: entry for k in keys}}))
PY
"$py" -m http.server "$port" --bind 127.0.0.1 --directory "$out/serve" >"$out/server.log" 2>&1 &
server=$!

echo "3. A, installed"
if [ "$os" = mac ]; then
  mkdir -p "$out/apps"
  ditto "$out/A/Lockdown.app" "$out/apps/Lockdown.app"
  app="$out/apps/Lockdown.app/Contents/MacOS/lockdown-desktop"
else
  "$out/A/Lockdown_${A}_x64-setup.exe" //S
  app=$(find "$LOCALAPPDATA" -maxdepth 3 -name lockdown-desktop.exe | head -n 1)
  [ -n "$app" ] || { echo "::error::A's installer put lockdown-desktop.exe nowhere under LOCALAPPDATA"; exit 1; }
fi
echo "A is at $app"

echo "4. A, started"
LOCKDOWN_UPDATE_TEST_ENDPOINT="http://127.0.0.1:$port/latest.json" \
LOCKDOWN_UPDATE_TEST_PUBKEY="$(cat "$out/key.pub")" \
LOCKDOWN_UPDATE_TEST_REPORT="$report" \
LOCKDOWN_UPDATE_TEST_AUTORESTART=1 \
LOCKDOWN_UPDATE_TEST_PASSWORD="$pw" \
  "$app" >"$out/app.log" 2>&1 &

stop() {
  if [ "$os" = mac ]; then pkill -f "Lockdown.app/Contents/MacOS/lockdown-desktop" || true
  else taskkill //IM lockdown-desktop.exe //F >/dev/null 2>&1 || true; fi
  kill "$server" 2>/dev/null || true
}
trap stop EXIT

echo "5. waiting for B"
for _ in $(seq 1 180); do
  if grep -q "^opened $B items=1" "$report"; then break; fi
  sleep 2
done
echo "--- report"; cat "$report"; echo "---"
grep -q "^started $A enabled=true install=InPlace" "$report" || { echo "::error::A didn't start as an updatable build"; exit 1; }
grep -q "^opened $A items=1" "$report" || { echo "::error::A didn't open the vault"; exit 1; }
grep -q "^state $A Ready offered=$B" "$report" || { echo "::error::A never had B downloaded and checked"; exit 1; }
grep -q "^restarting" "$report" || { echo "::error::A didn't restart to update"; exit 1; }
grep -q "^started $B enabled=true" "$report" || { echo "::error::B never started"; exit 1; }
grep -q "^opened $B items=1" "$report" || { echo "::error::B didn't open the vault"; exit 1; }
grep -q "^state $B UpToDate" "$report" || echo "note: B hadn't finished its own check yet"
stop
trap - EXIT
sleep 3

echo "6. the vault, read again by the CLI"
lockdown list | tee "$out/after.txt"
grep -q "Before the update" "$out/after.txt" || { echo "::error::the item isn't in the vault after the update"; exit 1; }
if [ "$os" = mac ]; then
  plist="$out/apps/Lockdown.app/Contents/Info.plist"
  have=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$plist" 2>/dev/null || true)
  echo "installed bundle says $have"
fi
echo "update end to end passed: A ($A) updated itself to B ($B), and B opened the vault"
