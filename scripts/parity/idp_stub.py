#!/usr/bin/env python3
"""Minimal OpenID Connect discovery + JWKS endpoint for the Basket parity harness.

Identity.API only supports the authorization-code grant, so a script cannot mint a
real token. This stub publishes the JWKS for an ephemeral RSA key the harness
generates, and both the .NET and the Rust basket service are pointed at it via
``Identity__Url`` — so both validate the harness's tokens the same way the real
Identity.API tokens are validated.

Usage: idp_stub.py <port> <public-key-modulus-hex> <kid>
"""

import base64
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer


def b64url(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).decode().rstrip("=")


def main() -> int:
    port = int(sys.argv[1])
    modulus_hex = sys.argv[2]
    kid = sys.argv[3]

    issuer = f"http://localhost:{port}"
    modulus = bytes.fromhex(modulus_hex)
    jwks = {
        "keys": [
            {
                "kty": "RSA",
                "use": "sig",
                "alg": "RS256",
                "kid": kid,
                "n": b64url(modulus),
                "e": b64url((65537).to_bytes(3, "big")),
            }
        ]
    }
    discovery = {
        "issuer": issuer,
        "jwks_uri": f"{issuer}/.well-known/openid-configuration/jwks",
        "authorization_endpoint": f"{issuer}/connect/authorize",
        "token_endpoint": f"{issuer}/connect/token",
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
    }

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):  # noqa: N802 - http.server API
            if self.path.startswith("/.well-known/openid-configuration/jwks"):
                body = jwks
            elif self.path.startswith("/.well-known/openid-configuration"):
                body = discovery
            else:
                self.send_error(404)
                return

            payload = json.dumps(body).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *_args):
            pass

    HTTPServer(("127.0.0.1", port), Handler).serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
