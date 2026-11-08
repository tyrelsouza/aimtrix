#!/usr/bin/env bash
# Add the licenses of the libraries an AppImage bundles to it.
#
#   dev/appimage-notices.sh src-tauri/target/release/bundle/appimage
#
# The AppImage carries GTK, WebKitGTK, GStreamer and ~130 other libraries
# from the build machine, many under the LGPL, which must come with their
# licenses and a way to get their source. Run this on the Ubuntu machine that
# built it, right after `tauri build` (it asks dpkg where each library came
# from): it writes usr/share/doc/aimtrix/BUNDLED-LIBRARIES.txt into the
# AppDir Tauri leaves behind, repacks the AppImage with the tool Tauri used,
# and leaves a copy next to it (<AppImage>.licenses.txt) for releases.
set -euo pipefail

dir="$1"
appdir="$(echo "$dir"/*.AppDir)"
image="$(echo "$dir"/*.AppImage)"
[ -d "$appdir" ] && [ -f "$image" ] || { echo "No AppDir and AppImage in $dir" >&2; exit 1; }
notice="$appdir/usr/share/doc/aimtrix/BUNDLED-LIBRARIES.txt"
mkdir -p "$(dirname "$notice")"

# The Ubuntu package each bundled library came from.
packages="$(
  find "$appdir/usr/lib" -name '*.so*' \( -type f -o -type l \) -printf '%f\n' | sort -u |
    while read -r lib; do
      dpkg -S "*/$lib" 2>/dev/null | awk -F': ' -v lib="/$lib" '
        { n = length(lib); if (substr($2, length($2) - n + 1) == lib) { split($1, p, ", "); for (i in p) print p[i] } }' || true
    done | sed 's/:amd64$//' | sort -u
)"
[ -n "$packages" ] || { echo "Found no packages for the bundled libraries (is this the build machine?)" >&2; exit 1; }

{
  cat <<'EOF'
AIMtrix AppImage: bundled libraries
===================================

AIMtrix itself is MIT licensed: see LICENSE.txt and THIRD-PARTY-LICENSES.html
in usr/lib/aimtrix, or About AIMtrix in Preferences. To run on most Linux
systems, this AppImage also
contains the libraries below, unmodified, taken from Ubuntu 22.04. AIMtrix
links to them dynamically, so you can replace any of them with your own
build. Many are under the GNU LGPL; their full license and copyright notices
follow the list.

Their complete source code is available from Ubuntu, at the link given for
each one (Launchpad keeps every published version).

EOF
  for pkg in $packages; do
    dpkg-query -W -f='${Package} ${Version}  (source: ${source:Package} ${source:Version})\n' "$pkg"
    src="$(dpkg-query -W -f='${source:Package}' "$pkg")"
    srcver="$(dpkg-query -W -f='${source:Version}' "$pkg")"
    echo "    https://launchpad.net/ubuntu/+source/$src/$srcver"
  done
  echo
  echo "Copyright and license notices"
  echo "============================="
  for pkg in $packages; do
    copyright="$(readlink -f "/usr/share/doc/$pkg/copyright" || true)"
    echo
    echo "---------------------------------------------------------------------"
    echo "$pkg"
    echo "---------------------------------------------------------------------"
    if [ -f "$copyright" ]; then cat "$copyright"; else echo "(no copyright file shipped)"; fi
  done
} > "$notice"
command cp -f "$notice" "$image.licenses.txt"

# Repack with the appimagetool inside the plugin Tauri used (it caches it on
# first build), reusing the original's runtime (the start of the file)
# instead of letting appimagetool download one.
plugin="$(echo "${XDG_CACHE_HOME:-$HOME/.cache}"/tauri/linuxdeploy-plugin-appimage*.AppImage)"
[ -x "$plugin" ] || { echo "No linuxdeploy-plugin-appimage in the Tauri cache" >&2; exit 1; }
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
(cd "$work" && APPIMAGE_EXTRACT_AND_RUN=1 "$plugin" --appimage-extract >/dev/null)
tool="$(find "$work/squashfs-root" -name appimagetool -type f | head -1)"
offset="$(APPIMAGE_EXTRACT_AND_RUN=1 "$image" --appimage-offset)"
head -c "$offset" "$image" > "$work/runtime"
# Into a new file first: if repacking fails, the original stays, and so does
# the tool's output to say why.
ARCH=x86_64 "$tool" --no-appstream --runtime-file "$work/runtime" "$appdir" "$work/repacked.AppImage" \
  >"$work/appimagetool.log" 2>&1 ||
  { cat "$work/appimagetool.log" >&2; echo "Repacking the AppImage failed" >&2; exit 1; }
mv -f "$work/repacked.AppImage" "$image"
chmod +x "$image"
echo "Added the licenses of $(echo "$packages" | wc -w) bundled packages to $(basename "$image")"
