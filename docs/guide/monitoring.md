# Health checks and monitoring

## `/healthz` and `/readyz`

Two endpoints tell an orchestrator or a load balancer about the server. Neither
needs credentials, and both sit outside `/v1` because they are not part of the
versioned API.

| Endpoint       | Question it answers                  | `200` when                          | Otherwise                              |
| -------------- | ------------------------------------ | ----------------------------------- | -------------------------------------- |
| `GET /healthz` | Is the process alive?                | The process is up and answering     | No answer: restart the process         |
| `GET /readyz`  | Can it take requests right now?      | The store is open, not shutting down | `503 INTERNAL`: send no traffic for now |

```bash
curl -s http://127.0.0.1:7878/healthz
# {"success":true,"data":{"status":"ok"},"error":null}
curl -s http://127.0.0.1:7878/readyz
# {"success":true,"data":{"status":"ready"},"error":null}
```

- Use `/healthz` for **liveness**: restart the container if it stops answering.
- Use `/readyz` for **readiness**: route traffic only while it answers `200`. It
  turns `503` as soon as a shutdown starts, so a load balancer stops sending new
  requests while in-flight ones finish.
- `idbseqd healthcheck` calls `/healthz` on the configured address and exits 0
  when it answers `200`. The Docker image uses it as its `HEALTHCHECK`. It speaks
  plain HTTP; with `[tls]`, use an HTTPS probe instead.

::: tip Why the "z"?
The `z` does not stand for a word. It comes from Google's internal servers,
where diagnostic pages were named `/statusz`, `/varz` and `/healthz` ("z-pages").
Kubernetes made `/healthz`, `/readyz` and `/livez` widely known. The odd
suffix keeps these routes from clashing with real application paths such as
`/health` or `/status`, and marks them as operational endpoints rather than API.
:::

## Metrics

Turn on Prometheus/OpenMetrics text at `GET /metrics`:

```toml
[metrics]
enabled = true
```

| Metric                                              | Type      | Labels       | Meaning                                                        |
| --------------------------------------------------- | --------- | ------------ | -------------------------------------------------------------- |
| `idbseq_ids_issued_total`                           | counter   | `collection` | IDs handed out                                                 |
| `idbseq_http_requests_total`                        | counter   | `status`     | HTTP requests by response status                               |
| `idbseq_http_request_duration_seconds`              | histogram |              | Request latency (`_bucket`, `_sum`, `_count`)                  |
| `idbseq_auth_failures_total`                        | counter   |              | Requests rejected for missing or invalid credentials           |
| `idbseq_collection_remaining_capacity_ratio`        | gauge     | `collection` | Share of the ID range not yet issued: 1 = unused, 0 = exhausted |

`/metrics` needs no credentials, so expose it only to your monitoring network
(for example, keep the server on a private address and let only the proxy and
Prometheus reach it).

```yaml
# prometheus.yml
scrape_configs:
  - job_name: idbseq
    static_configs:
      - targets: ['idbseq.internal:7878']
```

Useful alerts:

```yaml
groups:
  - name: idbseq
    rules:
      - alert: IdbSeqCollectionAlmostExhausted
        expr: idbseq_collection_remaining_capacity_ratio < 0.1
        for: 15m
        annotations:
          summary: 'Collection {{ $labels.collection }} has less than 10% of its IDs left'
      - alert: IdbSeqAuthFailures
        expr: rate(idbseq_auth_failures_total[5m]) > 1
        for: 10m
        annotations:
          summary: 'Repeated authentication failures: a wrong or revoked key?'
      - alert: IdbSeqServerErrors
        expr: sum(rate(idbseq_http_requests_total{status=~"5.."}[5m])) > 0
        for: 5m
```

## Logs and request IDs

- Logs are **JSON lines on stdout**. Set the level with `log` in the
  configuration or `IDBSEQ_LOG` (a tracing filter such as `info` or
  `idbseq_server=debug`).
- Every response carries an **`X-Request-Id`** header. Send your own to have it
  echoed back; otherwise the server generates one. The same ID appears in the
  logs, in error responses' `requestId` in the Node.js SDK, and in the audit log,
  so one value ties a client error to the server's record of it.

## Shutdown

`SIGTERM` or `SIGINT` stops accepting connections, turns `/readyz` to `503`, lets
in-flight requests finish for up to 30 seconds, and exits 0. Give your process
manager a stop timeout a little above that (35 s in the
[deployment examples](./deployment.md)). A `SIGKILL` or a power loss is also safe:
every ID returned to a caller is already on disk.
