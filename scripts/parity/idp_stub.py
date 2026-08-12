#!/usr/bin/env python3
"""Minimal OpenID Connect metadata server for the basket parity harness.

Serves just enough of Identity.API's surface (discovery document + JWKS) for a basket service to
validate RS256 tokens the harness mints, so parity runs do not need the real identity server, its
database, or a browser login.
"""

import argparse
import base64
import json
import subprocess
from http.server import BaseHTTPRequestHandler, HTTPServer


def rsa_public_jwk(pem_path: str, kid: str) -> dict:
    modulus_line = subprocess.run(
        ["openssl", "rsa", "-in", pem_path, "-noout", "-modulus"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    modulus = bytes.fromhex(modulus_line.removeprefix("Modulus=")).lstrip(b"\x00")

    def b64url(raw: bytes) -> str:
        return base64.urlsafe_b64encode(raw).decode().rstrip("=")

    return {
        "kty": "RSA",
        "use": "sig",
        "alg": "RS256",
        "kid": kid,
        "n": b64url(modulus),
        # openssl genrsa always uses the standard public exponent 65537.
        "e": b64url((65537).to_bytes(3, "big")),
    }


def build_handler(issuer: str, jwk: dict):
    discovery = {
        "issuer": issuer,
        "jwks_uri": f"{issuer}/.well-known/openid-configuration/jwks",
        "authorization_endpoint": f"{issuer}/connect/authorize",
        "token_endpoint": f"{issuer}/connect/token",
        "response_types_supported": ["code", "token", "id_token"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "scopes_supported": ["openid", "profile", "basket"],
    }
    jwks = {"keys": [jwk]}

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):  # noqa: N802 - required by BaseHTTPRequestHandler
            if self.path.startswith("/.well-known/openid-configuration/jwks"):
                self.respond(jwks)
            elif self.path.startswith("/.well-known/openid-configuration"):
                self.respond(discovery)
            else:
                self.send_error(404)

        def respond(self, payload: dict):
            body = json.dumps(payload).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_args):
            pass

    return Handler


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--key", required=True, help="PEM file holding the RSA signing key")
    parser.add_argument("--kid", default="parity-key")
    args = parser.parse_args()

    issuer = f"http://127.0.0.1:{args.port}"
    handler = build_handler(issuer, rsa_public_jwk(args.key, args.kid))
    HTTPServer(("127.0.0.1", args.port), handler).serve_forever()


if __name__ == "__main__":
    main()
