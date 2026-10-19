#!/usr/bin/env python3
"""Refresh the XJ-03 Spotify Portal token and persist the rotation atomically.

Why this exists (2026-10-19)
---------------------------
The Spotify Portal authorization server ROTATES refresh tokens. Every
successful ``refresh_token`` grant invalidates the refresh token it
consumed and returns a new one. The grant succeeds with HTTP 200 while
the *old* token is already dead.

That makes a naive refresh dangerous. If a refresh is performed with curl
and the response is not written back to ``tokens.json``, the cache is left
holding a spent refresh token. The next launch then fails with::

    {"error":"invalid_grant","error_description":"Invalid refresh token"}

which looks like a broken authorization server but is really a local
cache-consistency bug. This was observed live on 2026-10-19.

This helper performs the refresh and writes the rotated pair back into
``tokens.json`` in the same operation, so the cache never holds a spent
refresh token. It is idempotent: running it repeatedly leaves the cache
in a refreshable state every time.

It never prints token material.

Usage
-----
    python3 scripts/refresh-portal-token.py            # refresh + persist
    python3 scripts/refresh-portal-token.py --check    # report only, no refresh

Exit codes
----------
    0 = refresh succeeded and the rotated tokens were persisted
    1 = the refresh failed (the cached refresh token was already spent)
    2 = no token cache found
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

TOKEN_PATH = os.path.expanduser(
    "~/.mcp-auth/mcp-remote-v1/abb316d3ba70bc7220e11618a1b43906_tokens.json"
)
TOKEN_URL = "https://phenotype.spotifyportal.com/api/auth/v1/token"
CLIENT_ID = "https://auth.kooshapari.com/oauth/client-metadata.json"
RESOURCE = "https://phenotype.spotifyportal.com/api/mcp-actions/v1"


def post_form(url: str, fields: dict[str, str]) -> tuple[int, dict]:
    """POST an application/x-www-form-urlencoded body and decode the JSON reply."""
    body = urllib.parse.urlencode(fields).encode()
    request = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": "application/x-www-form-urlencoded"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, json.loads(response.read().decode())
    except urllib.error.HTTPError as exc:
        raw = exc.read().decode()
        try:
            return exc.code, json.loads(raw)
        except json.JSONDecodeError:
            return exc.code, {"error": "unparseable_response", "raw": raw[:200]}


def describe(cached: dict) -> str:
    """Summarize the cached token set without revealing any token material."""
    expires_at = cached.get("expires_at")
    left = "unknown"
    if isinstance(expires_at, (int, float)):
        left = "%ds" % int(expires_at / 1000 - time.time())
    return (
        "access_token=<%d chars> refresh_token=<%d chars> expires_in=%s left=%s scope=%s"
        % (
            len(cached.get("access_token", "")),
            len(cached.get("refresh_token", "")),
            cached.get("expires_in"),
            left,
            cached.get("requested_scope") or cached.get("scope"),
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="report the cached token state without performing a refresh",
    )
    parser.add_argument(
        "--token-path",
        default=TOKEN_PATH,
        help="override the tokens.json path (default: %(default)s)",
    )
    args = parser.parse_args()
    token_path = os.path.expanduser(args.token_path)

    if not os.path.exists(token_path):
        print("FAIL: no token cache at %s" % token_path)
        return 2

    with open(token_path) as handle:
        cached = json.load(handle)

    if args.check:
        print("cache: %s" % describe(cached))
        return 0

    refresh_token = cached.get("refresh_token")
    if not refresh_token:
        print("FAIL: cached token set has no refresh_token")
        return 2

    status, body = post_form(
        TOKEN_URL,
        {
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLIENT_ID,
            "resource": RESOURCE,
            "scope": cached.get("requested_scope") or cached.get("scope") or "",
        },
    )

    if status != 200 or not body.get("access_token"):
        print(
            "FAIL: refresh returned HTTP %s (%s). The cached refresh token was "
            "already spent; re-run the consent flow to mint a new one."
            % (status, body.get("error"))
        )
        return 1

    # Persist the rotated pair BEFORE reporting success. The refresh token
    # just consumed is now spent and must not remain in the cache.
    cached["access_token"] = body["access_token"]
    if body.get("refresh_token"):
        cached["refresh_token"] = body["refresh_token"]
    if body.get("id_token"):
        cached["id_token"] = body["id_token"]
    if body.get("expires_in"):
        cached["expires_in"] = body["expires_in"]
        cached["expires_at"] = int(time.time() * 1000) + int(body["expires_in"]) * 1000

    tmp_path = token_path + ".tmp"
    with open(tmp_path, "w") as handle:
        json.dump(cached, handle, indent=2)
    os.replace(tmp_path, token_path)
    os.chmod(token_path, 0o600)

    print("OK: refreshed (HTTP 200); rotated refresh token persisted")
    print("    cache: %s" % describe(cached))
    return 0


if __name__ == "__main__":
    sys.exit(main())
