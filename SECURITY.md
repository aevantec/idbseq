# Security policy

## Reporting a vulnerability

Please report security problems **privately**, not in public issues or pull requests.

Use GitHub's private vulnerability reporting: open the repository's **Security** tab
and choose **Report a vulnerability**. Include what you found, how to reproduce it,
the affected version (`idbseqd --version`), and the impact you expect.

We aim to acknowledge a report within 3 working days and to agree on a fix and a
disclosure date with you within 14 days. Credit is given in the release notes unless
you ask otherwise.

## Supported versions

Until 1.0, only the latest minor release receives security fixes.

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |
| < 0.1   | No        |

## Scope

In scope: `idbseqd` (HTTP API, authentication, API keys, protected collections,
the local admin socket, backup/restore), the `idbseq` CLI, the Node.js and PHP SDKs,
and the published Docker image.

Particularly interesting: issuing the same ID twice, bypassing authentication,
scopes, collection allow-lists or collection protection, and leaking secrets
(passwords, session tokens, API keys) through responses or logs.

Out of scope: deployments that ignore [docs/operations.md](docs/operations.md), such
as binding to a public address without TLS or a TLS proxy, and denial of service
from clients that hold valid credentials and stay within the configured rate limits.
