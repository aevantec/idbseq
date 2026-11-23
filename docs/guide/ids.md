# IDs, order and gaps

## What an ID is

An ID is a **signed 64-bit integer**, from the collection's `start` up to its
`max` (at most `9223372036854775807`). On the wire it is a plain JSON number,
never a string:

```json
{ "success": true, "data": { "id": 1044 }, "error": null }
```

::: warning JavaScript and big numbers
JavaScript's `JSON.parse` silently rounds integers above 2^53 (9007199254740991).
The [Node.js SDK](../clients/node.md) parses responses losslessly and returns a
`bigint` above that limit. Python `int`, Go `int64`, Java `long` and PHP `int`
(64-bit builds) hold every ID exactly.
:::

## Taking an ID versus looking at one

| Call                                          | CLI                         | Effect                                                    |
| --------------------------------------------- | --------------------------- | --------------------------------------------------------- |
| `POST /v1/collections/{name}/next`            | `idbseq next`               | **Takes** the next ID: the counter moves on               |
| `POST /v1/collections/{name}/next?count=N`    | `idbseq next --count N`     | **Takes** N consecutive IDs at once                       |
| `GET /v1/collections/{name}/last`             | `idbseq last`               | Shows `last_id` and `next_id`; takes nothing              |
| `GET /v1/collections/{name}`                  | `idbseq collections info`   | Shows the settings plus `last_id` and `next_id`           |

`next` hands you the ID **for good**. No other caller will ever get it, even if
you never use it, and the next call returns the following one.

The `next_id` shown by `last` and `info` is only a **preview**. It is not
reserved: another client may take it a moment later. Never use `next_id` as an
ID; always call `next`.

## Batches

`?count=N` takes N IDs in one request and returns a range:

```json
{ "success": true, "data": { "first": 1045, "last": 1144, "count": 100, "step": 1 } }
```

The IDs are `first`, `first + step`, … up to `last`. A batch is all or nothing:
if it does not fit below `max`, nothing is issued and you get
`SEQUENCE_EXHAUSTED`. `count` is 1–10,000 by default (`limits.max_batch`).
`count=1` also returns this range shape; only a request **without** `count`
returns `{ "id" }`.

Batches are the fastest way to get many IDs: one request and one disk commit for
the whole range. Take a batch when you are about to create many records, and
hand the IDs out from memory.

## Order

Within a collection, every ID is **greater than every ID issued before it**,
including across restarts and crashes. Two callers asking at the same moment get
different IDs; which one gets the smaller is not defined.

Order is per collection. IDs from different collections are unrelated.

## Gaps are normal

idbseq guarantees **uniqueness and order, not gaplessness**
([ADR-0004](../adr/0004-gap-policy.md)). A number is skipped when:

- a response is lost (a timeout, a dropped connection) after the server issued
  the ID, and the client asks again without an
  [idempotency key](./retries.md);
- a client takes an ID or a batch and does not use all of it;
- a collection uses [block durability](./durability.md#block-mode) and the app
  or server stops: the unused rest of the reserved block is skipped;
- someone [resets](./collections.md#reset) the counter forward.

Never try to fill or reuse a gap: the skipped numbers may already be in use
somewhere you cannot see. If you need numbering with no gaps (for example,
legally required invoice numbers), assign those numbers inside your own database
transaction and use idbseq IDs as internal keys.

## Never a duplicate

In the default `strict` mode, every ID is committed to disk **before** it is
returned. After a crash, a power loss or a `kill -9`, idbseq continues above
the last ID it handed out. In `block` mode, it continues above the end of the
last reserved block. Either way, an ID is never issued twice.

The one way to get a duplicate is to move a counter **backwards** on purpose:
a reset with `allow_rewind`, deleting and recreating a collection, or restoring
an old backup with `--force` without skipping ahead. Each needs an explicit
step, and each is audited.
