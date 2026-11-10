#!/usr/bin/env bash
# Check commit subjects against the repo convention: `<type>: <description>`
# with type one of feat, fix, refactor, docs, test, chore, perf, ci.
#
# Usage:
#   scripts/check-commit-msg.sh --file <commit-msg-file>   # commit-msg hook
#   scripts/check-commit-msg.sh --range <rev-range>         # CI, e.g. origin/main..HEAD
set -euo pipefail

TYPES="feat|fix|refactor|docs|test|chore|perf|ci"
PATTERN="^(${TYPES}): [^ ].*"

check() {
  local subject="$1" where="$2"
  if [[ ! "$subject" =~ $PATTERN ]]; then
    echo "error: ${where}: \"${subject}\"" >&2
    echo "       expected \`<type>: <description>\` with type one of ${TYPES//|/, }" >&2
    return 1
  fi
}

case "${1:-}" in
  --file)
    subject="$(grep -v '^#' "$2" | head -n 1)"
    check "$subject" "commit message"
    ;;
  --range)
    failed=0
    while IFS=$'\t' read -r sha subject; do
      check "$subject" "commit ${sha}" || failed=1
    done < <(git log --format='%h%x09%s' "$2")
    exit "$failed"
    ;;
  *)
    echo "usage: $0 --file <commit-msg-file> | --range <rev-range>" >&2
    exit 2
    ;;
esac
