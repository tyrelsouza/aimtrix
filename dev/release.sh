#!/usr/bin/env bash
# Start a release: tag HEAD as v<next version> (dev/set-version.mjs) and
# push the tag. GitHub Actions (.github/workflows/release.yml) then builds
# the Linux packages and the macOS .dmg and attaches them to a draft
# release to review and publish on GitHub.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -n "$(git status --porcelain)" ]; then
  echo "There are uncommitted changes; commit them first, so the release matches a commit."
  exit 1
fi
git fetch --quiet origin
# Releases come from main, which CI checks (it runs only there).
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" != main ]; then
  echo "Releases come from main, and this is $BRANCH. Merge it first."
  exit 1
fi
if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
  echo "HEAD isn't origin/main; pull or push first."
  exit 1
fi

VERSION="$(node dev/set-version.mjs)"
TAG="v$VERSION"
echo "Release $TAG from $(git log -1 --format='%h %s')"
read -r -p "Tag it and push the tag? [y/N] " answer
[ "$answer" = "y" ] || [ "$answer" = "Y" ] || { echo "Nothing done."; exit 0; }

git tag -a "$TAG" -m "AIMtrix $VERSION"
git push origin "$TAG"
echo "Pushed $TAG. Watch the build with: gh run watch"
echo "Then review the draft at: https://github.com/tyrelsouza/aimtrix/releases"
