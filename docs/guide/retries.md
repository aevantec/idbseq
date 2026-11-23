# Retries and idempotency

Networks fail. When a `next` request times out, you cannot tell whether the
server issued the ID or not. Asking again without care burns a number (a gap)
or, worse, makes your code believe it has two IDs for one record.

## Idempotency keys

Send an `Idempotency-Key` header with every `next` request you might retry:

```bash
curl -sS -X POST -H "Authorization: Bearer $IDBSEQ_KEY" -H 'Content-Length: 0' \
  -H "Idempotency-Key: order-import-7f3a" \
  "$IDBSEQ_URL/v1/collections/orders/next?count=3"
```

If idbseq already handled a request with the same key, collection and
`count` from the **same credentials** (embedded: in the same file), it returns the **original** `data`
without issuing new IDs. So you can retry until you get an answer and still end
up with exactly one set of IDs.

- The key is any 1–255 visible ASCII characters. Use something unique per
  logical operation: a UUID, or your own record's identifier.
- Keys are remembered for **24 hours** (`limits.idempotency_ttl_hours`).
- The same key with a different collection or `count` is refused with
  `VALIDATION_FAILED`, so a reused key cannot silently return the wrong IDs.
- Keys are scoped to the credentials that sent them: two API keys never share
  idempotency records.

From the clients:

| Client  | How                                                                 |
| ------- | ------------------------------------------------------------------- |
| CLI     | `idbseq next orders --idempotency-key order-import-7f3a`            |
| Node.js | `ids.generateId('orders', { idempotencyKey: 'order-import-7f3a' })` |
| MCP     | `next_id` with `idempotency_key`                                    |

## What the SDK does for you

The Node.js SDK makes every `generateId` / `generateIds` call safe to retry on
its own:

- each call sends an `Idempotency-Key`: a random UUID, or yours if you pass one;
- network errors, `5xx` responses and `429` are retried with **the same key**,
  with exponential backoff (4 attempts, 100 ms doubling up to 2 s by default);
- a `Retry-After` header is honoured, up to 30 s.

Other calls are not retried. Pass your own key when a retry may come from a
different process, for example after a crash and restart of your worker: a
random per-call key cannot survive that, your own record ID can.

## Rate limiting

Each credential may make `per_second` requests per second with bursts up to
`burst` (defaults 10,000 and 20,000; `[rate_limit]` in the
[configuration](../operations.md#configuration)). Over the limit, requests get
`429 RATE_LIMITED` with a `Retry-After` header in seconds. Wait that long, then
retry with the same idempotency key. Set `per_second = 0` to turn the limit off.

## Timeouts

The server answers `408 REQUEST_TIMEOUT` if a request takes longer than
`limits.request_timeout_secs` (default 10 s). Clients should use a timeout of
their own a little above that. The Node.js SDK defaults to 10 s (`timeoutMs`)
and the CLI to 30 s (`--timeout`).

## Which errors to retry

| Error                                      | Retry?                                                |
| ------------------------------------------ | ----------------------------------------------------- |
| Network error, timeout, `5xx`              | Yes, with the same idempotency key                    |
| `429 RATE_LIMITED`                         | Yes, after `Retry-After`, with the same key           |
| `409 SEQUENCE_EXHAUSTED`                   | No: raise the collection's `max` first                |
| `401 UNAUTHORIZED`, `403 FORBIDDEN`        | No: fix the key or its scopes                         |
| `400 VALIDATION_FAILED`, `404 …NOT_FOUND`  | No: fix the request                                   |

All error codes are listed in the [HTTP API reference](../API.md#error-codes).
