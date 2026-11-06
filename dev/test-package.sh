#!/usr/bin/env bash
# Install a built AIMtrix .deb on a clean Debian-family container and check
# it runs: Sign On shows, signing on to the dev homeserver works (and saves
# the session in a keyring), and a restart resumes into the Buddy List.
#
#   dev/build-package.sh                    # or: npm run tauri build
#   dev/test-package.sh                     # ubuntu:24.04
#   dev/test-package.sh debian:12           # or another image
#
# Needs Docker and the dev homeserver running. Screenshots and the app's log
# go to dev/package-test/<image>/. Signs on as a throwaway user, deactivated
# at the end so no devices are left behind.
set -euo pipefail

IMAGE="${1:-ubuntu:24.04}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# The portable build from dev/build-package.sh if there is one, else the
# host's own build.
DEB="$(ls -t "$ROOT"/dist/*.deb "$ROOT"/src-tauri/target/release/bundle/deb/*.deb 2>/dev/null | head -1)"
OUT="$ROOT/dev/package-test/${IMAGE//[:\/]/-}"
mkdir -p "$OUT"
rm -f "$OUT"/*.png "$OUT"/*.log
echo "Testing $(basename "$DEB") on $IMAGE; results in $OUT"

# A throwaway user (shared-secret registration, like dev/seed.py).
USER_NAME="pkgtest-$RANDOM$RANDOM"
python3 - "$USER_NAME" <<'PY'
import hashlib, hmac, json, sys, urllib.request
hs, user = "http://localhost:8008", sys.argv[1]
url = hs + "/_synapse/admin/v1/register"
nonce = json.load(urllib.request.urlopen(url))["nonce"]
mac = hmac.new(b"aimtrix-dev-secret", f"{nonce}\0{user}\0aimtrix-test\0notadmin".encode(), hashlib.sha1)
body = {"nonce": nonce, "username": user, "password": "aimtrix-test", "admin": False, "mac": mac.hexdigest()}
req = urllib.request.Request(url, json.dumps(body).encode(), {"Content-Type": "application/json"})
urllib.request.urlopen(req)
PY
deactivate() {
  python3 - "$USER_NAME" <<'PY'
import json, sys, urllib.request
hs, user = "http://localhost:8008", sys.argv[1]
def call(path, body, token=None):
    req = urllib.request.Request(hs + path, json.dumps(body).encode(), {"Content-Type": "application/json"})
    if token:
        req.add_header("Authorization", "Bearer " + token)
    return json.load(urllib.request.urlopen(req))
token = call("/_matrix/client/v3/login", {"type": "m.login.password", "identifier": {"type": "m.id.user", "user": user}, "password": "aimtrix-test"})["access_token"]
auth = {"type": "m.login.password", "identifier": {"type": "m.id.user", "user": user}, "password": "aimtrix-test"}
call("/_matrix/client/v3/account/deactivate", {"auth": auth, "erase": False}, token)
PY
}
trap deactivate EXIT

docker run --rm --network host \
  -v "$DEB:/pkg/aimtrix.deb:ro" -v "$OUT:/out" \
  -e DEBIAN_FRONTEND=noninteractive -e USER_NAME="$USER_NAME" \
  "$IMAGE" bash -c '
set -euo pipefail
apt-get update -qq
# The package and its dependencies, plus a virtual display, a keyring and
# tools to type and take screenshots.
apt-get install -y -qq /pkg/aimtrix.deb xvfb xdotool imagemagick dbus-x11 \
  gnome-keyring >/out/apt.log 2>&1
# Tauri names the package after the product name ("ai-mtrix"), so ask.
dpkg -s "$(dpkg-deb -f /pkg/aimtrix.deb Package)" \
  | grep -E "^(Package|Version|Depends|Recommends):" | tee /out/package.txt

dbus-run-session -- bash -c "
  set -euo pipefail
  Xvfb :99 -screen 0 1024x768x24 >/dev/null 2>&1 &
  export DISPLAY=:99
  sleep 1
  echo -n test | gnome-keyring-daemon --unlock --components=secrets >/dev/null
  shot() { sleep \$1; import -window root /out/\$2.png; echo \"screenshot \$2\"; }

  aimtrix >/out/app.log 2>&1 &
  shot 6 1-sign-on
  # Release builds offer matrix.org and Custom...; the cursor starts in
  # Screen Name. Pick Custom..., which shows the Homeserver box.
  xdotool key shift+Tab; sleep 0.3; xdotool type c; sleep 0.5
  xdotool key Tab; sleep 0.3
  xdotool type --delay 20 http://localhost:8008
  xdotool key Tab; xdotool type --delay 20 \$USER_NAME
  xdotool key Tab; xdotool type --delay 20 aimtrix-test
  shot 1 2-filled-in
  xdotool key Return
  shot 12 3-signed-on

  # Quit and start again: the saved session should resume.
  pkill -x aimtrix || true
  sleep 2
  aimtrix >>/out/app.log 2>&1 &
  shot 12 4-resumed
  pkill -x aimtrix || true
"
'
echo "Done. Check the screenshots in $OUT."
