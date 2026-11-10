# ADR-0002: REST/JSON over HTTP only

- Status: Accepted (provisional; owner may revisit)
- Date: 2026-11-10
- Settled by: ISSUE-0003

## Context

Clients in any language must reach idbseq. Official SDKs exist only for Node.js
and PHP; every other language relies on plain HTTP or a generated client.

## Decision

v1 speaks REST with JSON bodies over HTTP/1.1 and HTTP/2, described by an
OpenAPI 3.1 contract generated from the server code (`api/openapi.json`). Every
response uses one envelope with stable error codes (`docs/API.md`).

## Consequences

- Any language with an HTTP client works; curl is enough to debug.
- Per-request overhead is higher than gRPC. High-volume clients use batches
  (`?count=n`) instead of one request per ID.

## Alternatives considered

- **gRPC:** faster and typed, but needs codegen and is awkward from PHP and browsers.
  Revisit only if a benchmark shows HTTP overhead matters.
- **Both REST and gRPC:** two contracts to keep in step.
