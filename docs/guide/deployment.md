# Deployment

`idbseqd` is one process with one data file. Deploying it means: keep the data
directory on durable storage, run **exactly one** process per data directory,
put TLS in front, and back it up. All settings are in the
[configuration reference](../operations.md#configuration).

::: warning One process per data directory
idbseq v1 is a single node ([ADR-0001](../adr/0001-storage-engine.md)). The data
directory is locked, and a second process on it exits with an error. Do not run
replicas or put two servers behind a load balancer: each would hand out the same
IDs from its own copy. Availability comes from fast restarts and
[backups](../operations.md#backup-and-restore).
:::

## Docker

The image `ghcr.io/aevantec/idbseq` (linux/amd64 and linux/arm64) runs `idbseqd`
as a non-root user (uid 65532) on a distroless base:

| Setting          | Value in the image                                      |
| ---------------- | ------------------------------------------------------- |
| Listens on       | `0.0.0.0:7878` (`IDBSEQ_BIND`)                          |
| Data             | `/data`, a volume (`IDBSEQ_DATA_DIR`)                   |
| Health check     | `idbseqd healthcheck` every 30 s                        |
| Also contains    | the `idbseq` CLI                                        |
| Tags             | `latest`, `0.1.1`, `0.1`, and the commit SHA            |

```yaml
# compose.yaml
services:
  idbseq:
    image: ghcr.io/aevantec/idbseq:0.1
    restart: unless-stopped
    ports:
      - '127.0.0.1:7878:7878' # only this host; put a TLS proxy in front
    volumes:
      - idbseq-data:/data
    environment:
      # Read only while no admin exists; remove after the first start.
      IDBSEQ_ADMIN_PASSWORD: ${IDBSEQ_ADMIN_PASSWORD}
    # In-flight requests get up to 30 s to finish on shutdown.
    stop_grace_period: 35s

volumes:
  idbseq-data:
```

Run server commands inside the container. While the server runs, `admin` and
`backup` reach it through a socket in the data directory:

```bash
docker compose exec idbseq idbseqd backup /data/idbseq-backup.redb
docker compose exec idbseq idbseqd admin reset-password
docker compose exec idbseq idbseq --help
```

To use a config file, mount it and point `IDBSEQ_CONFIG` at it
(`-v ./idbseq.toml:/etc/idbseq.toml:ro -e IDBSEQ_CONFIG=/etc/idbseq.toml`).

## systemd

```ini
# /etc/systemd/system/idbseqd.service
[Unit]
Description=idbseq ID sequence server
After=network-online.target
Wants=network-online.target

[Service]
User=idbseq
Group=idbseq
ExecStart=/usr/local/bin/idbseqd
Environment=IDBSEQ_CONFIG=/etc/idbseq/idbseq.toml
StateDirectory=idbseq
StateDirectoryMode=0700
Restart=on-failure
# SIGTERM drains in-flight requests for up to 30 s.
TimeoutStopSec=35

[Install]
WantedBy=multi-user.target
```

```toml
# /etc/idbseq/idbseq.toml
bind = "127.0.0.1:7878"
data_dir = "/var/lib/idbseq"
```

```bash
sudo useradd --system --home /var/lib/idbseq --shell /usr/sbin/nologin idbseq
sudo systemctl enable --now idbseqd # systemd creates /var/lib/idbseq
# While it runs, admin commands reach it through a socket in the data directory:
sudo -u idbseq IDBSEQ_CONFIG=/etc/idbseq/idbseq.toml idbseqd admin init
```

## TLS

API keys travel in a header, so anything beyond `localhost` needs HTTPS. Two
ways:

**A reverse proxy** (recommended when you already have one). For example, Caddy
gets and renews a certificate on its own:

```
ids.example.com {
	reverse_proxy 127.0.0.1:7878
}
```

**Built-in TLS** (rustls), when the server faces the network directly:

```toml
bind = "0.0.0.0:7878"

[tls]
cert = "/etc/idbseq/cert.pem"
key = "/etc/idbseq/key.pem"
```

Binding to a non-loopback address without `[tls]` logs a warning. With `[tls]`,
`idbseqd healthcheck` (plain HTTP) cannot check the server; use an HTTPS probe
such as `curl -fsS https://ids.example.com/healthz`.

Behind a proxy, the audit log's `client_ip` is the proxy's address: idbseq reads
the connection's peer address, not `X-Forwarded-For`. Rate limits are per
credential, so a proxy does not merge clients into one limit.

## Kubernetes

Run a **single replica** with a persistent volume. Use a StatefulSet (or a
Deployment with `strategy: Recreate`) so that an old pod has stopped before a new
one opens the data directory.

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: idbseq
spec:
  replicas: 1 # never more: see the warning above
  serviceName: idbseq
  selector:
    matchLabels: { app: idbseq }
  template:
    metadata:
      labels: { app: idbseq }
    spec:
      terminationGracePeriodSeconds: 35
      containers:
        - name: idbseqd
          image: ghcr.io/aevantec/idbseq:0.1
          ports:
            - containerPort: 7878
          volumeMounts:
            - { name: data, mountPath: /data }
          livenessProbe:
            httpGet: { path: /healthz, port: 7878 }
          readinessProbe:
            httpGet: { path: /readyz, port: 7878 }
  volumeClaimTemplates:
    - metadata:
        name: data
      spec:
        accessModes: [ReadWriteOnce]
        resources:
          requests:
            storage: 1Gi
```

Create the admin once with
`kubectl exec -it idbseq-0 -- idbseqd admin init`, then reach the API through a
Service and an Ingress that terminates TLS. See
[Health checks and monitoring](./monitoring.md) for what the probes mean.

## Checklist

- [ ] Data directory on durable storage, backed up regularly with
      `idbseqd backup` ([backup and restore](../operations.md#backup-and-restore)).
- [ ] Exactly one `idbseqd` per data directory.
- [ ] TLS in front of anything not on `localhost`.
- [ ] `IDBSEQ_ADMIN_PASSWORD` removed after the first start.
- [ ] One API key per application, with the fewest
      [scopes](./security.md#api-key-scopes) and an allow-list.
- [ ] Important collections marked [protected](./collections.md#protected-collections).
- [ ] [Health checks and metrics](./monitoring.md) wired into your monitoring.
