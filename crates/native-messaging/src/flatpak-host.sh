#!/bin/sh
# Lockdown's browser host for the Lockdown Flatpak
# (docs/plan/29-release-polish.md §4). The Lockdown app writes this file
# into its own data folder when a browser is connected, and the browser's
# native-messaging manifest names it. It runs on the host, outside the
# sandbox, where the browser starts it.
#
# The host program has to run inside the app's own sandbox: the app checks
# the program that connects by its process (same user, its path and its
# SHA-256), and a process in another sandbox is invisible to it. So while
# the app runs, `flatpak enter` starts the host in the running app's
# namespaces. When the app isn't running, a new sandbox starts the host,
# which answers that Lockdown isn't running.
app='@APP_ID@'
flatpak=$(command -v flatpak 2>/dev/null || echo /usr/bin/flatpak)
if "$flatpak" ps --columns=application 2>/dev/null | grep -qx "$app"; then
  exec "$flatpak" enter "$app" /app/bin/lockdown-native-host "$@"
fi
exec "$flatpak" run --command=lockdown-native-host "$app" "$@"
