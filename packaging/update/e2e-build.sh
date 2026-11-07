#!/usr/bin/env bash
# Builds the two desktop apps the end-to-end update runs between
# (docs/plan/60-auto-update.md §7), for ci.yml's `update-e2e` job on macOS
# and Windows: A at 0.1.0-main.20260101000000 and B at
# 0.1.0-main.20260102000000, both debug builds with the `update-test`
# feature (the test-only endpoint and key, and the run's hooks), each signed
# for the updater with the job's throwaway key; and the CLI that makes and
# reads the run's vault.
#
#   packaging/update/e2e-build.sh <dir holding the throwaway key as "key">
set -euo pipefail

out=$(cd "$1" && pwd)
root=$(cd "$(dirname "$0")/../.." && pwd)
TAURI_SIGNING_PRIVATE_KEY="$(cat "$out/key")"
export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

case "$(uname -s)" in
  Darwin) bundles=app; export APPLE_SIGNING_IDENTITY=- ;;
  MINGW* | MSYS* | CYGWIN*) bundles=nsis ;;
  *) echo "e2e-build: macOS and Windows only" >&2; exit 2 ;;
esac

cd "$root/apps/desktop"
for build in A:20260101000000 B:20260102000000; do
  name=${build%%:*}
  version="0.1.0-main.${build#*:}"
  # The bundler checks the signature it makes against plugins.updater.pubkey.
  printf '{"version":"%s","plugins":{"updater":{"pubkey":"%s"}}}\n' "$version" "$(tr -d '[:space:]' <"$out/key.pub")" >"$out/version-$name.json"
  pnpm tauri build --debug --ci --bundles "$bundles" --features update-test \
    --config src-tauri/tauri.updater.conf.json --config "$out/version-$name.json"
  mkdir -p "$out/$name"
  if [ "$bundles" = app ]; then
    ditto "$root/target/debug/bundle/macos/Lockdown.app" "$out/$name/Lockdown.app"
    cp "$root/target/debug/bundle/macos/Lockdown.app.tar.gz" "$out/$name/Lockdown_${version}.app.tar.gz"
    cp "$root/target/debug/bundle/macos/Lockdown.app.tar.gz.sig" "$out/$name/Lockdown_${version}.app.tar.gz.sig"
  else
    cp "$root/target/debug/bundle/nsis/Lockdown_${version}_x64-setup.exe" "$out/$name/"
    cp "$root/target/debug/bundle/nsis/Lockdown_${version}_x64-setup.exe.sig" "$out/$name/"
  fi
  ls -la "$out/$name"
done

# The test-only marker is in these builds, so the release check's needle is real.
bin="$root/target/debug/lockdown-desktop"
[ -f "$bin.exe" ] && bin="$bin.exe"
grep -a -q -F "lockdown-update-test-3b9d" "$bin" || { echo "::error::the update-test marker isn't in the update-test build"; exit 1; }
"$root/packaging/devtools/check.sh" present "$bin"

cd "$root"
cargo build -p lockdown-cli --bin lockdown
cp target/debug/lockdown* "$out/" 2>/dev/null || true
ls -la "$out"
