#!/usr/bin/env bash
# Start a throwaway idbseqd and run every end-to-end client suite against it:
# the Node SDK, the MCP server, the idbseq CLI, the other-language examples
# and the admin console (US-0046, in a headless browser).
#
# Usage: scripts/e2e.sh [node] [mcp] [cli] [examples] [ui]   (default: all)
# Needs: cargo; plus npm for the SDK suites (installed deps), the
# Playwright browser for ui (`npx playwright install chromium` in
# crates/idbseq-server/ui), and
# whichever example toolchains you have (missing ones are skipped by examples/run.sh).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SUITES=("$@")
[[ ${#SUITES[@]} -eq 0 ]] && SUITES=(node mcp cli examples ui)
PASSWORD="e2e admin password $$"
PORT="${IDBSEQ_E2E_PORT:-17878}"
URL="http://127.0.0.1:${PORT}"
DATA_DIR="$(mktemp -d)"
SERVER_PID=""
CONSOLE_PID=""

cleanup() {
  [[ -n "$CONSOLE_PID" ]] && kill -TERM "$CONSOLE_PID" 2>/dev/null || true
  if [[ -n "$SERVER_PID" ]]; then
    kill -TERM "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$DATA_DIR"
}
export IDBSEQ_CONFIG_DIR="$DATA_DIR/cli-config"   # keep the CLI away from ~/.config
trap cleanup EXIT

cargo build --locked --quiet --manifest-path "$ROOT/Cargo.toml" --bin idbseqd --bin idbseq
BIN="$ROOT/target/debug"

printf 'ui = true\n' >"$DATA_DIR/idbseq.toml"   # serve the console for the ui suite
IDBSEQ_CONFIG="$DATA_DIR/idbseq.toml" IDBSEQ_DATA_DIR="$DATA_DIR" IDBSEQ_BIND="127.0.0.1:${PORT}" IDBSEQ_ADMIN_PASSWORD="$PASSWORD" \
  IDBSEQ_LOG=warn "$BIN/idbseqd" &
SERVER_PID=$!
for _ in $(seq 1 100); do
  curl -fsS "$URL/healthz" >/dev/null 2>&1 && break
  sleep 0.1
done
curl -fsS "$URL/healthz" >/dev/null

admin_token() {
  curl -fsS "$URL/v1/auth/login" -H 'Content-Type: application/json' \
    -d "{\"username\":\"admin\",\"password\":\"${PASSWORD}\"}" | sed -E 's/.*"token":"([^"]+)".*/\1/'
}

run_node() {
  (cd "$ROOT/sdk/node" && IDBSEQ_E2E_URL="$URL" IDBSEQ_E2E_ADMIN_PASSWORD="$PASSWORD" npm run --silent test:e2e)
}

run_mcp() {
  (cd "$ROOT/sdk/mcp" && IDBSEQ_E2E_URL="$URL" IDBSEQ_E2E_ADMIN_PASSWORD="$PASSWORD" npm run --silent test:e2e)
}

# `idbseqd console` on a new file, for the ui suite's local.spec.ts. Sets CONSOLE_LINK.
start_console() {
  "$BIN/idbseqd" console --create "$DATA_DIR/app.idbseq" >/dev/null 2>"$DATA_DIR/console.log" &
  CONSOLE_PID=$!
  for _ in $(seq 1 100); do
    grep -q '^http://' "$DATA_DIR/console.log" 2>/dev/null && break
    sleep 0.1
  done
  CONSOLE_LINK="$(grep -m1 '^http://' "$DATA_DIR/console.log")"
}

run_ui() {
  start_console
  (cd "$ROOT/crates/idbseq-server/ui" && IDBSEQ_E2E_URL="$URL" IDBSEQ_E2E_ADMIN_PASSWORD="$PASSWORD" \
    IDBSEQ_E2E_CONSOLE_LINK="$CONSOLE_LINK" npm run --silent test:e2e)
}

run_cli() {
  local cli=("$BIN/idbseq" --url "$URL")
  local token
  token="$(admin_token)"
  IDBSEQ_KEY="$token" "${cli[@]}" collections create e2e-cli --start 100 >/dev/null
  local reply
  reply="$(IDBSEQ_KEY="$token" "${cli[@]}" --json next e2e-cli | tr -d ' \n')"
  if [[ "$reply" != '{"id":100}' ]]; then
    echo "cli: wanted {\"id\":100}, got ${reply}" >&2
    return 1
  fi
  IDBSEQ_KEY="$token" "${cli[@]}" collections delete e2e-cli --yes >/dev/null
  echo "cli ok"
}

run_examples() {
  local token key
  token="$(admin_token)"
  curl -fsS "$URL/v1/collections" -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"name":"examples"}' >/dev/null
  key="$(curl -fsS "$URL/v1/keys" -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{"name":"examples","scopes":["read","generate"],"collections":["examples"]}' |
    sed -E 's/.*"key":"([^"]+)".*/\1/')"
  IDBSEQ_URL="$URL" IDBSEQ_KEY="$key" "$ROOT/examples/run.sh"
}

for suite in "${SUITES[@]}"; do
  echo "== e2e: $suite"
  "run_${suite}"
done
echo "== e2e: all passed (${SUITES[*]})"
