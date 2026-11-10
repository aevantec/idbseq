#!/usr/bin/env bash
# idbseq from the shell with curl only (no jq needed).
#
# Environment:
#   IDBSEQ_URL         server base URL (default http://127.0.0.1:7878)
#   IDBSEQ_KEY         API key, required; needs the `read` and `generate` scopes
#   IDBSEQ_COLLECTION  an existing collection (default `examples`)
#
# Runs: generate one ID, generate a batch of 3, read the last ID, list
# collections (following meta.next_cursor). Exits non-zero on any failure.
set -euo pipefail

IDBSEQ_URL="${IDBSEQ_URL:-http://127.0.0.1:7878}"
IDBSEQ_URL="${IDBSEQ_URL%/}"
IDBSEQ_COLLECTION="${IDBSEQ_COLLECTION:-examples}"
PAGE_LIMIT=100

if [[ -z "${IDBSEQ_KEY:-}" ]]; then
  echo "error: IDBSEQ_KEY is not set (an API key with read + generate scopes)" >&2
  exit 2
fi

# json_str JSON FIELD: best-effort extraction of a string field from compact
# JSON. Good enough for error codes and cursors (collection names); use jq or
# a real parser for anything else.
json_str() {
  sed -n 's/.*"'"$2"'"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' <<<"$1"
}

# api METHOD PATH [curl args...]: prints the envelope on 2xx. Otherwise prints
# `error: CODE (HTTP status): message` to stderr and returns 1. A missing or
# unknown code is reported as HTTP_<status>.
api() {
  local method="$1" path="$2" out status body code message
  shift 2
  if ! out="$(curl -sS --max-time 10 -X "$method" \
    -H "Authorization: Bearer ${IDBSEQ_KEY}" \
    -H 'Accept: application/json' \
    -w '\n%{http_code}' "$@" "${IDBSEQ_URL}${path}")"; then
    echo "error: NETWORK_ERROR: ${method} ${IDBSEQ_URL}${path} failed" >&2
    return 1
  fi
  status="${out##*$'\n'}"
  body="${out%$'\n'*}"
  if [[ "$status" != 2?? ]]; then
    code="$(json_str "$body" code)"
    message="$(json_str "$body" message)"
    echo "error: ${code:-HTTP_${status}} (HTTP ${status}): ${message:-$body}" >&2
    return 1
  fi
  printf '%s\n' "$body"
}

base="/v1/collections/${IDBSEQ_COLLECTION}"

# Generate one ID -> data: { "id": 1044 }
# (Assign first: a failing $(...) inside echo would not stop the script.)
one="$(api POST "${base}/next" -H 'Content-Length: 0')"
echo "next:  ${one}"

# Generate a batch -> data: { "first", "last", "count", "step" }.
# A retry with the same Idempotency-Key returns the same batch.
idem="example-$(date +%s)-$$"
batch="$(api POST "${base}/next?count=3" -H 'Content-Length: 0' -H "Idempotency-Key: ${idem}")"
echo "batch: ${batch}"

# Last issued ID -> data: { "last_id", "next_id" } (either may be null)
last="$(api GET "${base}/last")"
echo "last:  ${last}"

# List collections, following meta.next_cursor until it is null.
cursor=""
while :; do
  args=(-G --data-urlencode "limit=${PAGE_LIMIT}")
  if [[ -n "$cursor" ]]; then
    args+=(--data-urlencode "cursor=${cursor}")
  fi
  page="$(api GET "/v1/collections" "${args[@]}")"
  echo "page:  ${page}"
  cursor="$(json_str "$page" next_cursor)"
  if [[ -z "$cursor" ]]; then
    break
  fi
done
