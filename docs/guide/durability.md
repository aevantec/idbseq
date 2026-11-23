# Durability and speed

Every collection chooses how its IDs reach the disk. The choice trades speed
against how many numbers a crash may skip. It never trades away uniqueness: in
both modes an ID is never handed out twice.

| Mode                 | Each ID is on disk…               | After a crash                         | Speed                      |
| -------------------- | --------------------------------- | ------------------------------------- | -------------------------- |
| `strict` (default)   | before it is returned             | continues after the last returned ID  | bounded by the disk's sync |
| `block` (size N)     | as part of a reserved block of N  | skips the unused rest of the block    | served from memory         |

## Strict mode

```json
{ "mode": "strict" }
```

The default. Every ID is committed (written and synced) before it is
returned, so a crash cannot lose a number that was already returned. The cost is one disk sync per commit, so speed depends on the disk's
sync latency.

**Group commit** keeps strict mode fast under load: requests that arrive while a
commit is running wait for the next one and share it, with a single sync for
all of them. It is always on. Tune it in the `[group_commit]` section of the
[configuration](../operations.md#configuration):

| Setting       | Default | Meaning                                                            |
| ------------- | ------- | ------------------------------------------------------------------ |
| `max_batch`   | `1024`  | The most ID requests that share one commit                         |
| `max_wait_us` | `0`     | Extra time to wait for more requests; `0` takes what is queued     |

## Block mode

```json
{ "mode": "block", "size": 1000 }
```

idbseq reserves `size` IDs on disk at once and then hands them out from
memory until the block is used up, with one commit per block instead of per
request. When the app or server stops, even cleanly, the unused rest of the
current block is skipped (a gap, never a duplicate).

Embedded, block mode applies in exclusive mode. A file opened in
[shared mode](./embedded.md#modes) writes block collections like strict ones,
because a reserved block would be lost at every close.

Use block mode for collections that need high request rates and can accept
gaps after a crash. Size the block **several times `count` × concurrent
callers**: with batches of 100 from 64 callers, a block of 1000 is too small and
is refilled constantly, which made it slower than strict mode in the benchmarks.

```bash
idbseq collections create events --durability block --block-size 10000
idbseq collections update events --durability strict
```

## Batches

`POST …/next?count=N` takes N IDs in one request and one commit, in either mode.
It is the fastest way to get many IDs: strict mode with group commit and
`count = 100` reached about **668,000 IDs per second** from 64 callers in the
benchmarks.

## Numbers from the reference machine

Measured on an Apple M1 Pro laptop with its internal SSD, where one durable sync
(`F_FULLFSYNC`) costs about 3–5 ms ([full method and results](../benchmarks.md),
[targets](../adr/0005-throughput-target.md)):

| Scenario                                        | Requests/s   | IDs/s      |
| ----------------------------------------------- | ------------ | ---------- |
| strict, one caller                              | ~185         | ~185       |
| strict + group commit, 64 callers               | ~6,800       | ~6,800     |
| strict + group commit, 64 callers, `count=100`  | ~6,700       | ~668,000   |
| block (1000), one caller                        | ~66,000      | ~66,000    |
| block (1000), 16–64 callers                     | ~100,000     | ~100,000   |
| over HTTP, strict, 64 callers                   | ~6,500       | ~6,500     |
| over HTTP, block (1000), loopback               | ~39,000      | ~39,000    |

These are laptop numbers. On a Linux server with an NVMe drive that has
power-loss protection, a sync is usually much cheaper, and strict mode much
faster. Run the [benchmarks](../benchmarks.md) on your own hardware before sizing.

## Choosing

- Start with **strict**. Most applications need far fewer than a few thousand
  new IDs per second.
- Need many IDs at once? Use **batches** before anything else.
- Need a very high *request* rate and can accept gaps after a crash? Use
  **block** with a generous size.
- Need no gaps at all? No mode gives you that; see
  [Gaps are normal](./ids.md#gaps-are-normal).
