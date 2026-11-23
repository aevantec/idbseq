# Running a server

Run `idbseqd` when **several machines, services or users** need IDs from one
sequence. It serves the same engine as [embedded idbseq](./getting-started.md)
over HTTP/JSON, adds API keys and an admin account, and works from any language.

This page takes you from nothing to your first IDs over the network: install the
server, create the admin, make a collection and an API key, and take IDs from
the command line, curl and the Node.js SDK.

::: tip Already embedding idbseq?
Stop your app, copy its file into the server's data directory as `idbseq.redb`,
and start `idbseqd`: your collections and counters carry over. See
[the file](./embedded.md#the-file).
:::

## 1. Install

You need `idbseqd` (the server) and, optionally, `idbseq` (the command-line
client). Both ship together.

::: code-group

```bash [Linux, macOS]
# Linux (x86_64/aarch64, gnu or musl) and macOS (x86_64/arm64).
# Pick the archive for your platform from the GitHub Release.
VERSION=v0.1.1 # the latest release: github.com/aevantec/idbseq/releases
TARGET=x86_64-unknown-linux-gnu
curl -fsSLO "https://github.com/aevantec/idbseq/releases/download/$VERSION/idbseq-$VERSION-$TARGET.tar.gz"
curl -fsSLO "https://github.com/aevantec/idbseq/releases/download/$VERSION/SHA256SUMS"
sha256sum --check --ignore-missing SHA256SUMS
tar xzf "idbseq-$VERSION-$TARGET.tar.gz"
sudo install "idbseq-$VERSION-$TARGET/idbseqd" "idbseq-$VERSION-$TARGET/idbseq" /usr/local/bin/
```

```powershell [Windows]
# x86_64 or aarch64. Pick the archive for your machine from the GitHub Release.
$Version = "v0.1.1" # the latest release: github.com/aevantec/idbseq/releases
$Target = "x86_64-pc-windows-msvc"
$Base = "https://github.com/aevantec/idbseq/releases/download/$Version"
Invoke-WebRequest "$Base/idbseq-$Version-$Target.zip" -OutFile "idbseq-$Version-$Target.zip"
Invoke-WebRequest "$Base/idbseq-$Version-$Target.zip.sha256" -OutFile "idbseq.sha256"
# Compare with the checksum file:
(Get-FileHash "idbseq-$Version-$Target.zip" -Algorithm SHA256).Hash
Get-Content idbseq.sha256
Expand-Archive "idbseq-$Version-$Target.zip" -DestinationPath .
# idbseqd.exe and idbseq.exe are in idbseq-$Version-$Target\
```

```bash [Docker]
docker volume create idbseq-data
docker run -d --name idbseq -p 7878:7878 \
  -v idbseq-data:/data \
  -e IDBSEQ_ADMIN_PASSWORD='choose-a-long-password' \
  ghcr.io/aevantec/idbseq:latest
# The image contains the CLI too:
docker exec -it idbseq idbseq --help
```

```bash [From source]
# Rust is pinned in rust-toolchain.toml; rustup installs it on first use.
git clone https://github.com/aevantec/idbseq.git && cd idbseq
cargo build --release
# Binaries: target/release/idbseqd and target/release/idbseq
```

:::

On macOS use `shasum -a 256 --check --ignore-missing SHA256SUMS` instead of
`sha256sum`.

On Windows, `idbseqd backup`, `idbseqd admin init` and `idbseqd admin
reset-password` work only while the server is stopped: they reach a running
server through a Unix socket, which the Windows build does not have yet.

## 2. Start the server and create the admin

The admin is the one person account that manages API keys and protected
collections ([more](./security.md#the-admin-account)). Create it once:

```bash
idbseqd admin init # asks for a password, at least 8 characters
idbseqd            # serves http://127.0.0.1:7878, data in ./data
```

Or set `IDBSEQ_ADMIN_PASSWORD` on the first start (the Docker example above does
this). It is only read while no admin exists, so remove it afterwards.

Prefer a browser? Start with the [admin console](./console.md) on and no admin:

```bash
idbseqd --ui
# idbseqd: no admin account yet. Open this link within 30 minutes to choose the admin password:
#
# http://127.0.0.1:7878/ui/#/setup?token=3f9c…
```

Open the link, choose the password, and you are signed in.

Check that it runs:

```bash
curl -s http://127.0.0.1:7878/healthz
# {"success":true,"data":{"status":"ok"},"error":null}
```

By default the server only listens on `127.0.0.1`. To serve other machines, see
[Deployment](./deployment.md).

## 3. Create a collection and an API key

Sign in as the admin with the CLI. The session lasts 15 minutes.

```bash
export IDBSEQ_URL=http://127.0.0.1:7878
idbseq login                              # asks for the admin password
idbseq collections create orders --start 1000
idbseq keys create billing --scope generate,read --collection orders
```

`keys create` prints the key, `idb_<id>_<secret>`, **once**. Store it like a
password; applications send it as `Authorization: Bearer <key>`. This key may
only take IDs from and read `orders`; see [scopes](./security.md#api-key-scopes).

## 4. Take IDs

```bash
export IDBSEQ_KEY=idb_...   # the key from step 3
idbseq next orders           # 1000
idbseq next orders --count 3 # 1001..1003
idbseq last orders           # last 1003, next 1004 (only a preview)
```

`next` **takes** IDs: each one is yours for good and is never handed out again.
`last` only looks. See [IDs, order and gaps](./ids.md).

The same from other clients:

::: code-group

```bash [curl]
curl -sS -X POST -H "Authorization: Bearer $IDBSEQ_KEY" -H 'Content-Length: 0' \
  "$IDBSEQ_URL/v1/collections/orders/next"
# {"success":true,"data":{"id":1004},"error":null}
```

```ts [Node.js]
import { IdbSeqClient } from 'idbseq';

const ids = new IdbSeqClient({
  url: process.env.IDBSEQ_URL,
  apiKey: process.env.IDBSEQ_KEY,
});

const id = await ids.generateId('orders'); // 1005
const batch = await ids.generateIds('orders', 100); // { first: 1006, last: 1105, … }
```

```python [Python]
import json, os, urllib.request

request = urllib.request.Request(
    f"{os.environ['IDBSEQ_URL']}/v1/collections/orders/next",
    method="POST",
    data=b"",
    headers={"Authorization": f"Bearer {os.environ['IDBSEQ_KEY']}"},
)
with urllib.request.urlopen(request) as response:
    print(json.load(response)["data"]["id"])  # 1106
```

:::

## Next steps

- Learn the model: [collections](./collections.md), [IDs and gaps](./ids.md),
  [durability](./durability.md) and [safe retries](./retries.md).
- Give each application its own key with the fewest scopes:
  [Admin, API keys and scopes](./security.md).
- Run it for real: [configuration](../operations.md), [deployment](./deployment.md),
  [health checks and monitoring](./monitoring.md).
- Prefer a browser for admin work? Turn on the [admin console](./console.md).
- Browse every endpoint in the [HTTP API reference](../API.md) or the
  [API explorer](/api-explorer.html){target="_self"}. Enter your server's URL
  at the top so its code samples point there. Your server sends no CORS
  headers, so to send requests from a browser, set `swagger_ui = true` and open
  `/docs` on the server itself.
