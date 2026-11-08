#!/usr/bin/env python3
"""A fake homeserver without simplified sliding sync (MSC4186), for checking
that AIMtrix refuses to sign on to one (PLAN.md 7.1).

    python3 dev/old-server.py          # listens on http://localhost:8010

Then sign on with Server "Custom…" and homeserver http://localhost:8010.
It answers /versions like a server with only the older sync and 404s
everything else, logging each request, so you can also see that no
/login was attempted.
"""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = 8010
VERSIONS = {
    "versions": ["v1.11", "v1.12"],
    "unstable_features": {"org.matrix.msc3575": True},
}


class Handler(BaseHTTPRequestHandler):
    def reply(self):
        if self.path.split("?")[0] == "/_matrix/client/versions":
            status, body = 200, VERSIONS
        else:
            status, body = 404, {"errcode": "M_UNRECOGNIZED", "error": "Unrecognized request"}
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    do_GET = do_POST = do_PUT = reply


if __name__ == "__main__":
    print(f"Fake old homeserver on http://localhost:{PORT}")
    # This computer only, like the dev containers' ports.
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
