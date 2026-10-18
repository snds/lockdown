#!/bin/sh
# A browser reaching the Lockdown app inside the Flatpak
# (docs/plan/29-release-polish.md §4), run by release.yml's `flatpak` job
# after the bundle is installed. POSIX sh.
#
# 1. Starts the installed app under a virtual display and waits for its
#    browser endpoint (the endpoint file in the app's data folder).
# 2. Writes the host-side wrapper exactly as the app's Connect does
#    (crates/native-messaging/src/flatpak-host.sh with the app id), and
#    runs it as a browser would: a known extension's origin as the
#    argument, framed JSON on stdin. The wrapper enters the running app's
#    sandbox, both ends check each other by process (user, path,
#    SHA-256), the app answers `ready`, then a `status` request.
# 3. Runs the host in a sandbox of its own instead (`flatpak run
#    --command`), which the app can't see into: that must not get `ready`.
#    This is why the wrapper enters the app's sandbox.
set -eu

app=design.snds.lockdown
origin=chrome-extension://dbdblnolpiehhlabpmhobmkbjipmebpe/
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
fail() { echo "browser-test: FAIL: $*" >&2; [ -f "$work/app.log" ] && tail -n 50 "$work/app.log" >&2; exit 1; }

# A native-messaging frame: a 4-byte little-endian length, then the JSON.
frame() {
  n=$(printf %s "$1" | wc -c)
  # shellcheck disable=SC2059
  printf "$(printf '\\%03o\\%03o\\%03o\\%03o' $((n & 255)) $(((n >> 8) & 255)) $(((n >> 16) & 255)) $(((n >> 24) & 255)))"
  printf %s "$1"
}

# A virtual display and a session bus for the app.
if ! command -v Xvfb >/dev/null 2>&1 || ! command -v dbus-daemon >/dev/null 2>&1; then
  if command -v dnf >/dev/null 2>&1; then
    dnf install -y -q xorg-x11-server-Xvfb dbus-daemon >/dev/null
  elif command -v apt-get >/dev/null 2>&1; then
    apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xvfb dbus >/dev/null
  fi
fi
Xvfb :77 -screen 0 1280x800x24 >"$work/xvfb.log" 2>&1 &
xvfb=$!
export DISPLAY=:77
bus=$(dbus-daemon --session --fork --print-address)
export DBUS_SESSION_BUS_ADDRESS="$bus"
unset WAYLAND_DISPLAY || true

# WebKit's own process sandbox needs the Flatpak portal, which a CI
# container has no session for; the browser channel doesn't involve WebKit.
flatpak run --env=WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 "$app" >"$work/app.log" 2>&1 &
appjob=$!
cleanup() { kill "$appjob" "$xvfb" 2>/dev/null || true; }
trap cleanup EXIT

endpoint=""
i=0
while [ $i -lt 120 ]; do
  endpoint=$(find "$HOME/.var/app/$app" -name native-host.json 2>/dev/null | head -n 1)
  [ -n "$endpoint" ] && break
  kill -0 "$appjob" 2>/dev/null || fail "the app exited before its browser endpoint started"
  sleep 1
  i=$((i + 1))
done
[ -n "$endpoint" ] || fail "no endpoint file under $HOME/.var/app/$app after 120 s"
data=$(dirname "$endpoint")
echo "browser-test: the app's endpoint is up: $endpoint"
cat "$endpoint"
flatpak ps --columns=instance,pid,child-pid,application || true

wrapper="$data/lockdown-native-host-flatpak"
sed "s/@APP_ID@/$app/" "$root/crates/native-messaging/src/flatpak-host.sh" >"$wrapper"
chmod 700 "$wrapper"

# Through the wrapper, as a browser starts it. The pause keeps stdin open
# until the answer is back.
out=$( { frame '{"id":1,"op":"status"}'; sleep 15; } | timeout 60 "$wrapper" "$origin" | tr -c '[:print:]' ' ' || true)
echo "browser-test: through the wrapper: $out"
echo "$out" | grep -q '"event":"ready"' || fail "the app didn't answer ready through the wrapper"
echo "$out" | grep -q '"paired":false' || fail "the app didn't answer status through the wrapper"

# A host in a sandbox of its own: the app can't see that process.
out=$( { frame '{"id":1,"op":"status"}'; sleep 5; } | timeout 60 flatpak run --command=lockdown-native-host "$app" "$origin" | tr -c '[:print:]' ' ' || true)
echo "browser-test: from another sandbox: $out"
if echo "$out" | grep -q '"event":"ready"'; then
  fail "a host in another sandbox was answered; the process check didn't run"
fi
echo "$out" | grep -Eq '"event":"(refused|untrusted_app)"' || fail "a host in another sandbox wasn't refused plainly: $out"
echo "browser-test: ok"
