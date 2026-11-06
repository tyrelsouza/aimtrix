#!/usr/bin/env bash
# Start a release: tag HEAD as v<next version> (dev/set-version.mjs) and
# push the tag. GitHub Actions (.github/workflows/release.yml) then builds
# the Linux packages and attaches them to a draft release to review and
# publish on GitHub.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -n "$(git status --porcelain)" ]; then
  echo "There are uncommitted changes; commit them first, so the release matches a commit."
  exit 1
fi
git fetch --quiet origin
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$(git rev-parse HEAD)" != "$(git rev-parse "origin/$BRANCH" 2>/dev/null || true)" ]; then
  echo "HEAD isn't pushed to origin/$BRANCH yet; push it first."
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
