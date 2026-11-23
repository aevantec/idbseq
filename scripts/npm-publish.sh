#!/usr/bin/env bash
# Publishes the packed npm packages of a release, in dependency order: the
# @idbseq/<platform> modules, then idbseq, then idbseq-mcp. Run by
# .github/workflows/release.yml: with --dry-run in `npm-pack` on every run (so
# a dry run of the release exercises this exact command), for real in
# `npm-publish`.
#
# Usage: scripts/npm-publish.sh <dir with the .tgz files> <version> <dist-tag> [--dry-run]
#
# A version already on npm is skipped, so a failed release can be re-run.
set -euo pipefail

fail() {
  echo "npm-publish: $*" >&2
  exit 1
}

[[ $# -ge 3 ]] || fail "usage: npm-publish.sh <dir> <version> <dist-tag> [--dry-run]"
[[ -d "$1" ]] || fail "not a directory: $1"
# Absolute paths: npm reads `dir/file.tgz` as a GitHub repository shorthand.
DIR="$(cd "$1" && pwd)"
VERSION="$2"
DIST_TAG="$3"
DRY_RUN="${4:-}"
[[ -z "${DRY_RUN}" || "${DRY_RUN}" == "--dry-run" ]] || fail "unknown option: ${DRY_RUN}"

flags=(--access public --tag "${DIST_TAG}")
if [[ -n "${DRY_RUN}" ]]; then
  flags+=(--dry-run)
else
  flags+=(--provenance)
fi

publish() {
  local tarball="$1" name="$2"
  [[ -f "${tarball}" ]] || fail "missing ${tarball}"
  if npm view "${name}@${VERSION}" version > /dev/null 2>&1; then
    echo "${name}@${VERSION} is already published"
    return
  fi
  npm publish "${tarball}" "${flags[@]}"
}

platforms=0
for tarball in "${DIR}"/idbseq-*-"${VERSION}".tgz; do
  target="$(basename "${tarball}" "-${VERSION}.tgz")"
  target="${target#idbseq-}"
  [[ "${target}" == "mcp" ]] && continue
  publish "${tarball}" "@idbseq/${target}"
  platforms=$((platforms + 1))
done
[[ "${platforms}" -eq 8 ]] || fail "expected 8 platform packages, found ${platforms}"
publish "${DIR}/idbseq-${VERSION}.tgz" idbseq
publish "${DIR}/idbseq-mcp-${VERSION}.tgz" idbseq-mcp
