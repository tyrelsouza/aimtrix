#!/usr/bin/env bash
# Build the macOS .dmg on a Mac, into dist/. It's universal (runs on Intel
# and Apple Silicon Macs) and unsigned: people open it the first time with
# right-click → Open, since Gatekeeper doesn't know it.
#
#   dev/build-dmg.sh          # or: mise run package-mac
#
# Numbered like releases (dev/set-version.mjs); the version is passed to
# Tauri for this build only, so no file changes.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[ "$(uname)" = Darwin ] || { echo "Build the .dmg on a Mac." >&2; exit 1; }
VERSION="${VERSION:-$(node dev/set-version.mjs)}"
echo "Building AIMtrix $VERSION for macOS"

rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
[ -d node_modules ] || npm ci --no-audit --no-fund
# CI=true skips styling the .dmg's Finder window, which needs Finder
# automation permission (and a desktop session).
CI=true npm run tauri build -- --target universal-apple-darwin --bundles dmg \
  --config "{\"version\":\"$VERSION\"}"

mkdir -p dist
rm -f dist/*.dmg
cp src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg dist/
echo "Packages in $ROOT/dist:"
ls -l dist/*.dmg
