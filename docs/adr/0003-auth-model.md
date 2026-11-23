# ADR-0003: One admin, scoped API keys, protected collections

- Status: Accepted (provisional; owner may revisit)
- Date: 2026-11-10
- Settled by: ISSUE-0006

## Context

Applications need credentials to generate and read IDs. Destructive operations
(reset, delete) on important collections must be limited to a person.

## Decision

- **One admin account** (username `admin`), created locally with
  `idbseqd admin init`. Password ≥ 8 characters (lowered from 12 by the owner on 2026-11-17), stored as an Argon2id hash. Signing
  in returns an opaque 256-bit session token (stored hashed, 15-minute TTL); 5 failed
  logins in 15 minutes lock the account for 15 minutes.
- **API keys** `idb_<id>_<secret>` for applications, created by the admin, shown once,
  stored as a SHA-256 hash of the secret (the secret is 256-bit random, so a slow hash
  adds nothing). Scopes: `read`, `generate`, `manage`; optional collection allow-list
  with `*` prefix matching.
- **Protected collections:** only the admin creates them, changes `protected` or
  `max`, resets or deletes them. Ordinary keys can still read and generate.
- **Unprotected collections:** any `manage` key may reset or delete them. Moving a
  counter backwards always needs `allow_rewind: true`.
- Every destructive and admin operation is written to an append-only audit log.

## Consequences

- Key checks are a SHA-256 and a constant-time compare: cheap enough for every request.
- A lost admin password is recovered only with local access to the data directory
  (`idbseqd admin reset-password`).

## Alternatives considered

- **Multiple admins / roles:** out of scope for v1 (US-0037, Won't Do).
- **OAuth/OIDC:** heavy for a single-binary service; can sit in front as a proxy.
