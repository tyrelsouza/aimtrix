#!/usr/bin/env python3
"""Sign out leftover test devices on the dev homeserver.

Tests that fail or are killed before their cleanup leave "AIMtrix" devices
behind on alice, bob and carol, and every encrypted message has to be
encrypted for each one, which slows tests down until they time out.

Removes devices named "AIMtrix" that haven't been seen for an hour (so a
running app keeps its session), and never touches other devices (Element).

  python3 dev/clean-devices.py            # remove them
  python3 dev/clean-devices.py --dry-run  # just count
"""
import json
import sys
import time
import urllib.request

HS = "http://localhost:8008"
USERS = ["alice", "bob", "carol"]
PASSWORD = "aimtrix-test"
KEEP_SEEN_WITHIN_MS = 60 * 60 * 1000


def call(method, path, body=None, token=None):
    req = urllib.request.Request(HS + path, method=method,
                                 data=json.dumps(body).encode() if body is not None else None)
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", "Bearer " + token)
    return json.load(urllib.request.urlopen(req))


def main():
    dry_run = "--dry-run" in sys.argv
    now = time.time() * 1000
    for user in USERS:
        login = call("POST", "/_matrix/client/v3/login", {
            "type": "m.login.password", "identifier": {"type": "m.id.user", "user": user},
            "password": PASSWORD, "initial_device_display_name": "clean-devices",
        })
        token, own = login["access_token"], login["device_id"]
        devices = call("GET", "/_matrix/client/v3/devices", token=token)["devices"]
        stale = [d["device_id"] for d in devices
                 if d["device_id"] != own and d.get("display_name") == "AIMtrix"
                 and now - (d.get("last_seen_ts") or 0) > KEEP_SEEN_WITHIN_MS]
        print(f"{user}: {len(devices) - 1} devices, {len(stale)} stale"
              + (" (dry run)" if dry_run else " removed"))
        if stale and not dry_run:
            call("POST", "/_matrix/client/v3/delete_devices", {
                "devices": stale,
                "auth": {"type": "m.login.password", "password": PASSWORD,
                         "identifier": {"type": "m.id.user", "user": user}},
            }, token)
        call("POST", "/_matrix/client/v3/logout", {}, token)


if __name__ == "__main__":
    main()
