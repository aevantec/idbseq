# Architecture decision records

Each ADR records one decision: the context, what was chosen and what it costs.
ADRs are never edited after acceptance except to change their status; a new ADR
supersedes an old one. Copy [`template.md`](template.md) to start a new one.

| ADR | Decision | Status | Settled by |
| --- | -------- | ------ | ---------- |
| [0001](0001-storage-engine.md) | Embedded redb, single node | Accepted (provisional) | ISSUE-0004 |
| [0002](0002-transport.md) | REST/JSON over HTTP only | Accepted (provisional) | ISSUE-0003 |
| [0003](0003-auth-model.md) | One admin account, scoped API keys, protected collections | Accepted (provisional) | ISSUE-0006 |
| [0004](0004-gap-policy.md) | Gaps allowed, duplicates never | Accepted (provisional) | ISSUE-0001 |
| [0005](0005-throughput-target.md) | Throughput and latency targets per durability mode | Accepted | US-0038 |
| [0006](0006-embedded-first.md) | Embedded first, the server optional | Accepted | US-0052 |
