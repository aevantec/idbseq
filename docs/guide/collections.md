# Collections

A **collection** is one named, independent counter. Create one for each thing you
number: `orders`, `invoices`, `tickets`, `eu.customers`. IDs from different
collections are unrelated; two collections can hand out the same number.

## Settings

| Setting      | Default                         | Meaning                                                                 |
| ------------ | ------------------------------- | ----------------------------------------------------------------------- |
| `name`       | required                        | 1–64 characters: `a-z`, `0-9`, `_`, `.`, `-`, starting with a letter or digit |
| `start`      | `1`                             | The first ID                                                            |
| `step`       | `1`                             | The increment between IDs; greater than 0                               |
| `max`        | `9223372036854775807` (2^63 − 1) | The largest ID; `start` must not be above it                           |
| `durability` | `{ "mode": "strict" }`          | How IDs are persisted; see [Durability and speed](./durability.md)          |
| `protected`  | `false`                         | Only the admin may reset, delete or change `max`; see below             |

Names are **lowercase** and case-sensitive. Dots are useful for grouping, and API
keys can be limited to a prefix such as `eu.*`.

With `start: 1000` and `step: 10`, a collection hands out 1000, 1010, 1020, and so
on. A collection also reports:

- `last_id`: the most recently issued ID, or `null` before the first;
- `next_id`: what the next call would return, or `null` when the collection is
  exhausted. This is a preview, not a reservation.

## Create, inspect and change

::: code-group

```bash [CLI]
idbseq collections create invoices --start 1000 --step 10
idbseq collections info invoices
idbseq collections list --prefix inv
idbseq collections update invoices --max 999999
```

```bash [HTTP]
curl -sS -X POST "$IDBSEQ_URL/v1/collections" \
  -H "Authorization: Bearer $IDBSEQ_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"name":"invoices","start":1000,"step":10}'
```

:::

Creating needs a key with the `manage` scope (or the admin). Rules for changes:

- `start` and `step` can only change **before the first ID** is issued.
- `max` can never go below `last_id`.
- `durability` can be changed at any time.

## Exhaustion

When the next ID would pass `max`, the collection is **exhausted**: requests get
`409 SEQUENCE_EXHAUSTED`, `next_id` is `null`, and nothing is issued. A batch that
does not fit is refused as a whole, not shortened. Raise `max` to continue.

The `idbseq_collection_remaining_capacity_ratio` metric shows how much of each
collection's range is left (1 = unused, 0 = exhausted), so you can alert well
before that happens. See [monitoring](./monitoring.md#metrics).

## Reset

A reset moves the counter so that the next ID is a value you choose (by default
`start`):

```bash
idbseq reset orders --value 250000
```

- The value must be within `[start, max]` and on the step grid:
  `(value - start) % step == 0`.
- **Moving forward** (a value above `last_id`) is always safe; it only skips
  numbers.
- **Moving backwards** can hand out IDs that were already issued, so it needs an
  explicit `--allow-rewind` (`"allow_rewind": true` over HTTP).
- The CLI asks you to type the collection name; over HTTP you send it as
  `"confirm": "<name>"`.

Every reset is written to the [audit log](./security.md#audit-log). A typical safe
use is skipping ahead after [restoring a backup](../operations.md#backup-and-restore).

## Delete

```bash
idbseq collections delete orders
```

Deleting removes the collection **and its counter**. If you later create a
collection with the same name, it starts again from its `start`, so old IDs can
come back. Delete only collections you no longer need. Like reset, it asks for
confirmation (`?confirm=<name>` over HTTP) and is audited.

## Protected collections

Mark a collection `protected` when its numbers matter: published order numbers,
anything other systems store. For a protected collection:

- only the **admin** may create it, change any of its settings (including
  `protected` and `max`), reset it or delete it;
- API keys, even with `manage`, get `403 PROTECTED_COLLECTION` for those
  operations;
- reading and taking IDs work as usual with `read` and `generate` keys.

```bash
idbseq login
idbseq collections create invoices --protected
```

A leaked or over-scoped API key can then never rewind or wipe it. See
[Admin, API keys and scopes](./security.md). Protection is a server feature: an
[embedded](./embedded.md) file keeps the flag, and it takes effect when a server
serves the file.
