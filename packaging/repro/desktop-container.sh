#!/usr/bin/env bash
# The inside of packaging/repro/build-desktop.sh: runs in the pinned
# ubuntu:22.04 image with the checkout at /src and the output at /out.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

# TLS for the snapshot archive first (its version doesn't reach the build),
# then every build package from the snapshot.
apt-get update -qq
apt-get install -y -qq --no-install-recommends ca-certificates >/dev/null
cat >/etc/apt/sources.list <<SOURCES
deb https://snapshot.ubuntu.com/ubuntu/$SNAPSHOT jammy main universe
deb https://snapshot.ubuntu.com/ubuntu/$SNAPSHOT jammy-updates main universe
deb https://snapshot.ubuntu.com/ubuntu/$SNAPSHOT jammy-security main universe
SOURCES
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  build-essential pkg-config curl xz-utils file git python3 \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev \
  libdbus-1-dev libxdo-dev libssl-dev >/dev/null
dpkg-query -W -f '${Package} ${Version}\n' gcc binutils libc6-dev libwebkit2gtk-4.1-dev zlib1g python3 | sed 's/^/build-desktop: /'

curl -fsSL "https://nodejs.org/dist/v$NODE/node-v$NODE-linux-x64.tar.xz" -o /tmp/node.tar.xz
echo "$NODE_SHA256  /tmp/node.tar.xz" | sha256sum -c - >/dev/null
tar -xJf /tmp/node.tar.xz -C /opt
export PATH="/opt/node-v$NODE-linux-x64/bin:$PATH"
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
corepack enable

curl -fsSL "https://static.rust-lang.org/rustup/archive/$RUSTUP/x86_64-unknown-linux-gnu/rustup-init" -o /tmp/rustup-init
echo "$RUSTUP_SHA256  /tmp/rustup-init" | sha256sum -c - >/dev/null
chmod +x /tmp/rustup-init
/tmp/rustup-init -y -q --profile minimal --default-toolchain "$RUST" --no-modify-path
export PATH="/root/.cargo/bin:$PATH"
# This exact release, not rust-toolchain.toml's moving "stable".
export RUSTUP_TOOLCHAIN="$RUST"
export CARGO_INCREMENTAL=0 CARGO_TERM_COLOR=never
export RUSTFLAGS="--remap-path-prefix=/src=/lockdown --remap-path-prefix=/root/.cargo=/cargo"
echo "build-desktop: $(rustc --version), node $(node --version)"

cd /src/apps/desktop
pnpm install --frozen-lockfile
node scripts/stage-host.mjs --release
pnpm tauri build --ci --bundles deb,rpm --config src-tauri/tauri.bundle.conf.json

cd /src
deb=$(ls target/release/bundle/deb/*.deb)
rpm=$(ls target/release/bundle/rpm/*.rpm)
python3 packaging/repro/normalize-package.py "$deb" "$rpm"
mkdir -p /out/stage
cp "$deb" "$rpm" /out/
# What the Flatpak packs: the same release binaries.
cp target/release/lockdown-desktop target/release/lockdown-native-host /out/stage/
cp apps/desktop/src-tauri/icons/128x128.png /out/stage/icon-128.png
cd /out
sha256sum ./*.deb ./*.rpm stage/* | sed 's#  \./#  #' >SHA256SUMS
