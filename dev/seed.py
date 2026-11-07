#!/usr/bin/env python3
"""Seed the local dev homeserver with test users and rooms for AIMtrix.

Safe to re-run: existing users are reused and rooms are found by an
`aimtrix.seed` state event instead of being created again.

Creates:
  users      alice, bob, carol (password: aimtrix-test)
  dm-plain   alice <-> bob DM, unencrypted, with messages
  dm-e2ee    alice <-> carol DM, encrypted (send messages from a real client)
  group-plain "Test Chat": all three, unencrypted, ~60 messages for scrollback,
             favourited by alice
  group-e2ee "Secret Chat": all three, encrypted
  invite     "Invite Test": carol invites alice, alice has not joined

Only the Python standard library is used.
"""

import base64
import hashlib
import hmac
import json
import os
import struct
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zlib

HS = os.environ.get("AIMTRIX_HS", "http://localhost:8008")
SHARED_SECRET = os.environ.get("AIMTRIX_SHARED_SECRET", "aimtrix-dev-secret")
# For servers without Synapse's admin API (continuwuity).
REGISTRATION_TOKEN = os.environ.get("AIMTRIX_REGISTRATION_TOKEN", "aimtrix-dev-token")
# For servers whose accounts live in Matrix Authentication Service: its
# admin client (dev/mas/config.yaml).
MAS_ADMIN_CLIENT = ("01JA1MTR1XDEVADM1NC11ENT00", "aimtrix-dev-admin-secret")
PASSWORD = os.environ.get("AIMTRIX_PASSWORD", "aimtrix-test")
SERVER_NAME = "localhost"

# username -> (display name, avatar RGB or None to test the default icon)
USERS = {
    "alice": ("Alice Anderson", (220, 60, 60)),
    "bob": ("Bob Brown", (60, 90, 220)),
    "carol": ("Carol Chen", None),
}

SEED_EVENT = "aimtrix.seed"


class MatrixError(Exception):
    def __init__(self, status, body):
        super().__init__(f"HTTP {status}: {body}")
        self.status = status
        self.body = body


def call(method, path, body=None, token=None, raw=None, content_type="application/json"):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(HS + path, data=data, method=method)
    if data is not None:
        req.add_header("Content-Type", content_type)
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read() or b"{}")
    except urllib.error.HTTPError as e:
        try:
            err = json.loads(e.read())
        except ValueError:
            err = {}
        raise MatrixError(e.code, err) from None


def q(s):
    return urllib.parse.quote(s, safe="")


def mxid(user):
    return f"@{user}:{SERVER_NAME}"


def wait_for_server():
    for _ in range(60):
        try:
            # Not /health: only Synapse has it.
            urllib.request.urlopen(HS + "/_matrix/client/versions").read()
            return
        except OSError:
            time.sleep(1)
    sys.exit(f"Homeserver at {HS} is not up. Run: docker compose -f dev/docker-compose.yml up -d")


def register(user):
    """Register `user`, with Synapse's shared-secret admin API or, on servers
    without it, a registration token, or in MAS when the server's accounts
    live there.

    Returns the new user's access token (registering also logs in), or None
    if the user already exists.
    """
    try:
        nonce = call("GET", "/_synapse/admin/v1/register")["nonce"]
    except MatrixError as e:
        if e.status != 404:
            raise
        return register_with_token(user)
    mac = hmac.new(SHARED_SECRET.encode(), digestmod=hashlib.sha1)
    mac.update(b"\x00".join([nonce.encode(), user.encode(), PASSWORD.encode(), b"notadmin"]))
    try:
        return call("POST", "/_synapse/admin/v1/register", {
            "nonce": nonce, "username": user, "password": PASSWORD,
            "admin": False, "mac": mac.hexdigest(),
        })["access_token"]
    except MatrixError as e:
        if e.body.get("errcode") == "M_USER_IN_USE":
            return None
        raise


def register_with_token(user):
    """Register through `/register` with a registration token: the first
    request starts a session, the second answers its token stage."""
    body = {"username": user, "password": PASSWORD,
            "initial_device_display_name": "aimtrix seed script"}
    try:
        call("POST", "/_matrix/client/v3/register", body)
        sys.exit(f"{HS} registered {user} without a registration token")
    except MatrixError as e:
        if e.body.get("errcode") == "M_USER_IN_USE":
            return None
        # Synapse behind MAS: accounts are made in MAS instead.
        if e.status == 403 and mas_issuer():
            return register_with_mas(mas_issuer(), user)
        if e.status != 401:
            raise
        session = e.body["session"]
    body["auth"] = {"type": "m.login.registration_token",
                    "token": REGISTRATION_TOKEN, "session": session}
    try:
        return call("POST", "/_matrix/client/v3/register", body)["access_token"]
    except MatrixError as e:
        # continuwuity checks the name only after the token.
        if e.body.get("errcode") == "M_USER_IN_USE":
            return None
        raise


def mas_issuer():
    """MAS's address if the server delegates accounts to it, else None."""
    try:
        return call("GET", "/_matrix/client/v1/auth_metadata")["issuer"]
    except MatrixError:
        return None


def register_with_mas(issuer, user):
    """Create the account in MAS with its admin API, then sign on through
    the homeserver (MAS answers that too)."""
    basic = base64.b64encode(":".join(MAS_ADMIN_CLIENT).encode()).decode()
    req = urllib.request.Request(
        issuer + "oauth2/token", method="POST",
        data=b"grant_type=client_credentials&scope=urn:mas:admin",
        headers={"Authorization": f"Basic {basic}",
                 "Content-Type": "application/x-www-form-urlencoded"})
    with urllib.request.urlopen(req) as resp:
        admin = json.loads(resp.read())["access_token"]

    def api(method, path, body):
        req = urllib.request.Request(
            issuer + "api/admin/v1/" + path, method=method, data=json.dumps(body).encode(),
            headers={"Authorization": f"Bearer {admin}", "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req) as resp:
                return json.loads(resp.read() or b"{}")
        except urllib.error.HTTPError as e:
            raise MatrixError(e.code, e.read().decode()) from None

    try:
        created = api("POST", "users", {"username": user})
    except MatrixError as e:
        if e.status == 409:
            return None
        raise
    api("POST", f"users/{created['data']['id']}/set-password",
        {"password": PASSWORD, "skip_password_check": True})
    return login(user)


def login(user):
    return call("POST", "/_matrix/client/v3/login", {
        "type": "m.login.password",
        "identifier": {"type": "m.id.user", "user": user},
        "password": PASSWORD,
        "initial_device_display_name": "aimtrix seed script",
    })["access_token"]


def solid_png(rgb, size=64):
    """A solid-colour PNG, built by hand so we need no imaging library."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    row = b"\x00" + bytes(rgb) * size
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(row * size))
            + chunk(b"IEND", b""))


def upload_png(token, rgb, name):
    return call("POST", f"/_matrix/media/v3/upload?filename={q(name)}", token=token,
                raw=solid_png(rgb), content_type="image/png")["content_uri"]


def setup_profile(user, token):
    display, rgb = USERS[user]
    call("PUT", f"/_matrix/client/v3/profile/{q(mxid(user))}/displayname",
         {"displayname": display}, token)
    if rgb:
        uri = upload_png(token, rgb, f"{user}.png")
        call("PUT", f"/_matrix/client/v3/profile/{q(mxid(user))}/avatar_url",
             {"avatar_url": uri}, token)


def existing_seed_rooms(token):
    """Map seed key -> room id for seed rooms this user has joined."""
    found = {}
    for room_id in call("GET", "/_matrix/client/v3/joined_rooms", token=token)["joined_rooms"]:
        try:
            content = call("GET", f"/_matrix/client/v3/rooms/{q(room_id)}/state/{SEED_EVENT}/", token=token)
            found[content.get("key")] = room_id
        except MatrixError:
            pass
    return found


def create_room(token, key, invite, name=None, direct=False, encrypted=False, avatar=None):
    initial_state = [{"type": SEED_EVENT, "state_key": "", "content": {"key": key}}]
    if encrypted:
        initial_state.append({"type": "m.room.encryption", "state_key": "",
                              "content": {"algorithm": "m.megolm.v1.aes-sha2"}})
    if avatar:
        initial_state.append({"type": "m.room.avatar", "state_key": "", "content": {"url": avatar}})
    body = {
        "preset": "trusted_private_chat" if direct else "private_chat",
        "invite": [mxid(u) for u in invite],
        "is_direct": direct,
        "initial_state": initial_state,
    }
    if name:
        body["name"] = name
    return call("POST", "/_matrix/client/v3/createRoom", body, token)["room_id"]


def join(token, room_id):
    call("POST", f"/_matrix/client/v3/join/{q(room_id)}", {}, token)


def add_direct(user, token, other, room_id):
    """Record room_id as a DM with `other` in user's m.direct account data."""
    path = f"/_matrix/client/v3/user/{q(mxid(user))}/account_data/m.direct"
    try:
        direct = call("GET", path, token=token)
    except MatrixError:
        direct = {}
    rooms = direct.setdefault(mxid(other), [])
    if room_id not in rooms:
        rooms.append(room_id)
        call("PUT", path, direct, token)


_txn = 0


def send(token, room_id, body, html=None):
    global _txn
    _txn += 1
    content = {"msgtype": "m.text", "body": body}
    if html:
        content.update(format="org.matrix.custom.html", formatted_body=html)
    call("PUT", f"/_matrix/client/v3/rooms/{q(room_id)}/send/m.room.message/"
                f"seed-{int(time.time() * 1000)}-{_txn}", content, token)


def main():
    wait_for_server()

    tokens = {}
    for user in USERS:
        token = register(user)
        if token:
            setup_profile(user, token)
        tokens[user] = token or login(user)
        print(f"user  {mxid(user):20} {'created' if token else 'exists'}")

    a, b, c = tokens["alice"], tokens["bob"], tokens["carol"]
    have = existing_seed_rooms(a)

    def ensure(key, make):
        if key in have:
            print(f"room  {key:20} exists  {have[key]}")
            return
        room_id = make()
        print(f"room  {key:20} created {room_id}")

    def dm_plain():
        room = create_room(a, "dm-plain", ["bob"], direct=True)
        join(b, room)
        add_direct("alice", a, "bob", room)
        add_direct("bob", b, "alice", room)
        send(a, room, "hey bob, you there?")
        send(b, room, "yep! what's up")
        send(a, room, "testing AIMtrix")
        send(b, room, "nice, this is **bold** and red",
             html="nice, this is <b>bold</b> and <font color=\"#ff0000\">red</font>")
        send(a, room, "brb")
        return room

    def dm_e2ee():
        room = create_room(a, "dm-e2ee", ["carol"], direct=True, encrypted=True)
        join(c, room)
        add_direct("alice", a, "carol", room)
        add_direct("carol", c, "alice", room)
        return room

    def group_plain():
        room = create_room(a, "group-plain", ["bob", "carol"], name="Test Chat",
                           avatar=upload_png(a, (60, 170, 90), "test-chat.png"))
        join(b, room)
        join(c, room)
        senders = [a, b, c]
        for i in range(1, 61):
            send(senders[i % 3], room, f"backlog message {i}")
        send(c, room, "hi everyone!")
        call("PUT", f"/_matrix/client/v3/user/{q(mxid('alice'))}/rooms/{q(room)}/tags/m.favourite",
             {"order": 0.5}, a)
        return room

    def group_e2ee():
        room = create_room(a, "group-e2ee", ["bob", "carol"], name="Secret Chat", encrypted=True)
        join(b, room)
        join(c, room)
        return room

    def invite():
        return create_room(c, "invite", ["alice"], name="Invite Test")

    ensure("dm-plain", dm_plain)
    ensure("dm-e2ee", dm_e2ee)
    ensure("group-plain", group_plain)
    ensure("group-e2ee", group_e2ee)
    # Alice never joins the invite room, so only Carol's side can find it.
    have.update({k: v for k, v in existing_seed_rooms(c).items() if k == "invite"})
    ensure("invite", invite)

    # Don't leave a stray "seed script" device on each account.
    for token in tokens.values():
        call("POST", "/_matrix/client/v3/logout", {}, token)

    print(f"\nDone. Log in at http://localhost:8080 as alice / bob / carol, password {PASSWORD!r}.")


if __name__ == "__main__":
    main()
