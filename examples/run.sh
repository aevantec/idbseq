#!/usr/bin/env bash
# Run every idbseq client example (curl, Python, Go, Java) against a server.
#
# Requirements:
#   IDBSEQ_URL         server base URL, required (e.g. http://127.0.0.1:7878)
#   IDBSEQ_KEY         API key, required; it needs the `read` and `generate` scopes
#                      (and must be allowed on the collection below)
#   IDBSEQ_COLLECTION  collection to use (default `examples`); it must already
#                      exist, these examples do not create it
#
# A language whose toolchain is missing is skipped (`SKIP <lang>: <tool> not found`).
# Exits non-zero if any example that ran failed.
set -euo pipefail

if [[ -z "${IDBSEQ_URL:-}" || -z "${IDBSEQ_KEY:-}" ]]; then
  echo "error: set IDBSEQ_URL and IDBSEQ_KEY (a key with read + generate scopes)" >&2
  exit 2
fi
export IDBSEQ_URL IDBSEQ_KEY
export IDBSEQ_COLLECTION="${IDBSEQ_COLLECTION:-examples}"

EXAMPLES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

passed=()
failed=()
skipped=()

# has_tool TOOL VERSION_ARG: the tool is on PATH and actually runs (macOS ships
# `java` stubs that fail when no JDK is installed).
has_tool() {
  command -v "$1" >/dev/null 2>&1 && "$1" "$2" >/dev/null 2>&1
}

# run_example LANG TOOL VERSION_ARG COMMAND...: COMMAND runs from the example's directory.
run_example() {
  local lang="$1" tool="$2" version_arg="$3"
  shift 3
  if ! has_tool "$tool" "$version_arg"; then
    echo "SKIP ${lang}: ${tool} not found"
    skipped+=("$lang")
    return 0
  fi
  echo "=== ${lang}"
  if (cd "${EXAMPLES_DIR}/${lang}" && "$@"); then
    passed+=("$lang")
  else
    echo "FAIL ${lang}" >&2
    failed+=("$lang")
  fi
}

run_example curl curl --version bash ./idbseq.sh
run_example python python3 --version python3 ./idbseq_example.py
run_example go go version go run .
run_example java java -version java IdbSeqExample.java

echo
echo "passed:  ${passed[*]:-none}"
echo "skipped: ${skipped[*]:-none}"
echo "failed:  ${failed[*]:-none}"
if ((${#failed[@]} > 0)); then
  exit 1
fi
