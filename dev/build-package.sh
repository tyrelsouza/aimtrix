#!/usr/bin/env bash
# Build the Linux packages (.deb and .AppImage) inside an Ubuntu 22.04
# container, so they run on Ubuntu 22.04, Debian 12 and anything newer.
# Built on a rolling distro instead, they need that distro's glibc (2.39+
# for the .deb, and the .AppImage bundles libraries needing even newer).
#
#   dev/build-package.sh        # packages land in dist/
#   dev/test-package.sh         # then check the .deb on a clean system
#
# Builds from a copy of the working tree (your node_modules and target are
# left alone). The Rust toolchain, npm cache and build output are kept in
# Docker volumes, so only the first build is slow (~20 min).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/dist"
# Numbered like releases (dev/set-version.mjs); written into the copy only.
VERSION="${VERSION:-$(node "$ROOT/dev/set-version.mjs")}"
echo "Building AIMtrix $VERSION"
mkdir -p "$OUT"

docker run --rm \
  -v "$ROOT:/src:ro" -v "$OUT:/out" \
  -v aimtrix-build-rustup:/root/.rustup \
  -v aimtrix-build-cargo:/root/.cargo \
  -v aimtrix-build-npm:/root/.npm \
  -v aimtrix-build-target:/build/src-tauri/target \
  -e DEBIAN_FRONTEND=noninteractive \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" -e VERSION="$VERSION" \
  ubuntu:22.04 bash -c '
set -euo pipefail
apt-get update -qq
# Tauri v2 Linux build dependencies, plus Node.
apt-get install -y -qq curl ca-certificates build-essential pkg-config file \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev libxdo-dev \
  libayatana-appindicator3-dev xz-utils >/dev/null
if ! command -v node >/dev/null || [ "$(node -v | cut -c2- | cut -d. -f1)" -lt 22 ]; then
  curl -fsSL https://deb.nodesource.com/setup_22.x | bash - >/dev/null
  apt-get install -y -qq nodejs >/dev/null
fi
if [ ! -x /root/.cargo/bin/cargo ]; then
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null
fi
export PATH="/root/.cargo/bin:$PATH"
rustup update stable >/dev/null 2>&1 || true

# A copy of the working tree, without anything built on the host.
mkdir -p /build
tar -C /src --exclude=./node_modules --exclude=./src-tauri/target \
  --exclude=./dist --exclude=./build --exclude=./.svelte-kit \
  --exclude=./dev/package-test -cf - . | tar -C /build -xf -
cd /build
node dev/set-version.mjs "$VERSION"
npm ci --no-audit --no-fund >/dev/null
# The cached target keeps old bundles; start without them.
rm -rf src-tauri/target/release/bundle
# AppImages need FUSE to build their own tooling; extract instead.
APPIMAGE_EXTRACT_AND_RUN=1 npm run tauri build

rm -f /out/*.deb /out/*.AppImage
cp src-tauri/target/release/bundle/deb/*.deb src-tauri/target/release/bundle/appimage/*.AppImage /out/
chown "$HOST_UID:$HOST_GID" /out/*
echo "glibc needed:"; objdump -T src-tauri/target/release/aimtrix | grep -o "GLIBC_[0-9.]*" | sort -uV | tail -1
'
echo "Packages in $OUT:"
ls -l "$OUT"
