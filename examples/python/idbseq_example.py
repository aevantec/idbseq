#!/usr/bin/env python3
"""idbseq from Python, standard library only (Python 3.8+).

Environment:
  IDBSEQ_URL         server base URL (default http://127.0.0.1:7878)
  IDBSEQ_KEY         API key, required; needs the `read` and `generate` scopes
  IDBSEQ_COLLECTION  an existing collection (default `examples`)

Runs: generate one ID, generate a batch of 3, read the last ID, list
collections (following meta.next_cursor). Exits non-zero on any failure.
IDs are JSON numbers; Python ints are arbitrary precision, so json.loads is lossless.
"""

import http.client
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid

DEFAULT_URL = "http://127.0.0.1:7878"
TIMEOUT_SECONDS = 10
PAGE_LIMIT = 100


class ApiError(Exception):
    """A failed call: HTTP status (0 for network errors), error code, message."""

    def __init__(self, status, code, message):
        super().__init__(f"{code} (HTTP {status}): {message}")
        self.status = status
        self.code = code
        self.message = message


class Client:
    def __init__(self, base_url, key):
        self.base_url = base_url.rstrip("/")
        self.key = key

    def call(self, method, path, query=None, headers=None):
        """Send a request and return the success envelope, or raise ApiError."""
        url = self.base_url + path
        if query:
            url += "?" + urllib.parse.urlencode(query)
        request = urllib.request.Request(
            url,
            method=method,
            data=b"" if method == "POST" else None,
            headers={
                "Authorization": f"Bearer {self.key}",
                "Accept": "application/json",
                **(headers or {}),
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
                status, raw = response.status, response.read()
        except urllib.error.HTTPError as err:  # non-2xx: the body is the error envelope
            status, raw = err.code, err.read()
        except (OSError, http.client.HTTPException) as err:  # URLError, timeouts, resets
            raise ApiError(0, "NETWORK_ERROR", str(getattr(err, "reason", err))) from err

        try:
            envelope = json.loads(raw)
        except ValueError:
            envelope = None
        if not isinstance(envelope, dict):
            raise ApiError(status, f"HTTP_{status}", raw.decode("utf-8", "replace")[:200])
        if 200 <= status < 300 and envelope.get("success") is True:
            return envelope
        error = envelope.get("error") or {}
        # Unknown or missing codes are still errors of this HTTP status.
        raise ApiError(status, error.get("code") or f"HTTP_{status}", error.get("message") or "")

    def generate(self, collection):
        path = f"/v1/collections/{urllib.parse.quote(collection, safe='')}/next"
        return self.call("POST", path)["data"]["id"]

    def generate_batch(self, collection, count, idempotency_key=None):
        path = f"/v1/collections/{urllib.parse.quote(collection, safe='')}/next"
        headers = {"Idempotency-Key": idempotency_key} if idempotency_key else None
        return self.call("POST", path, {"count": count}, headers)["data"]

    def last(self, collection):
        path = f"/v1/collections/{urllib.parse.quote(collection, safe='')}/last"
        return self.call("GET", path)["data"]

    def list_all(self, prefix=None):
        """Yield every collection, following meta.next_cursor."""
        cursor = None
        while True:
            query = {"limit": PAGE_LIMIT}
            if prefix:
                query["prefix"] = prefix
            if cursor:
                query["cursor"] = cursor
            envelope = self.call("GET", "/v1/collections", query)
            yield from envelope["data"]
            cursor = (envelope.get("meta") or {}).get("next_cursor")
            if not cursor:
                return


def main():
    key = os.environ.get("IDBSEQ_KEY")
    if not key:
        print("error: IDBSEQ_KEY is not set (an API key with read + generate scopes)", file=sys.stderr)
        return 2
    client = Client(os.environ.get("IDBSEQ_URL") or DEFAULT_URL, key)
    collection = os.environ.get("IDBSEQ_COLLECTION") or "examples"

    try:
        print(f"next:  id={client.generate(collection)}")
        batch = client.generate_batch(collection, 3, idempotency_key=f"example-{uuid.uuid4()}")
        print(f"batch: first={batch['first']} last={batch['last']} count={batch['count']} step={batch['step']}")
        last = client.last(collection)
        print(f"last:  last_id={last['last_id']} next_id={last['next_id']}")
        for item in client.list_all():
            print(f"list:  {item['name']} last_id={item['last_id']} next_id={item['next_id']}")
    except ApiError as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
