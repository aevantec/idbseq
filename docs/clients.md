# Using idbseq from other languages

idbseq is plain HTTP/JSON, so any language can use it. Node.js has an
official SDK ([`sdk/node`](../sdk/node), package `idbseq`). For everything else
this page shows curl, Python, Go and Java, plus how to generate a client from the OpenAPI
contract. The wire contract itself is [API.md](API.md). From a terminal or a
script, use the [`idbseq` CLI](#command-line-client-idbseq); AI agents can use
the [MCP server](#ai-agents-mcp-server).

Every example below is also a runnable program in [`examples/`](../examples):

| Language | File                                                                 | Run                           |
| -------- | -------------------------------------------------------------------- | ----------------------------- |
| curl     | [`examples/curl/idbseq.sh`](../examples/curl/idbseq.sh)               | `bash idbseq.sh`              |
| Python   | [`examples/python/idbseq_example.py`](../examples/python/idbseq_example.py) | `python3 idbseq_example.py` |
| Go       | [`examples/go/main.go`](../examples/go/main.go)                       | `go run .` (Go 1.22+)         |
| Java     | [`examples/java/IdbSeqExample.java`](../examples/java/IdbSeqExample.java) | `java IdbSeqExample.java` (17+) |

All of them use only the standard library (curl needs nothing but curl), read the
same environment variables, generate one ID, generate a batch of 3, read the last
ID, list all collections, and exit non-zero on any failure:

| Variable            | Meaning                                                           |
| ------------------- | ----------------------------------------------------------------- |
| `IDBSEQ_URL`        | Server base URL, default `http://127.0.0.1:7878`                  |
| `IDBSEQ_KEY`        | API key, **required**; needs the `read` and `generate` scopes     |
| `IDBSEQ_COLLECTION` | Collection to use, default `examples`; it must already exist      |

`examples/run.sh` runs all four (it requires `IDBSEQ_URL` and `IDBSEQ_KEY`), prints
`SKIP <lang>: <tool> not found` for a missing toolchain, and exits non-zero if any
example that ran failed:

```bash
# Once, with a key that has the `manage` scope (or an admin token):
curl -sS -X POST "$IDBSEQ_URL/v1/collections" \
  -H "Authorization: Bearer $IDBSEQ_MANAGE_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"name":"examples"}'

IDBSEQ_URL=http://127.0.0.1:7878 IDBSEQ_KEY=idb_... examples/run.sh
```

## What every client must do

- Send `Authorization: Bearer <key>` on every `/v1` call.
- Read the envelope `{ "success", "data", "error" }`. On failure print
  `error.code` and `error.message`. Treat a code you do not know (or a body that is
  not an envelope, e.g. from a proxy) as an error of the HTTP status.
- Handle network errors and timeouts separately from API errors.
- IDs are signed 64-bit **JSON numbers**, up to `9223372036854775807`. Python `int`,
  Go `int64` and Java `long` hold them exactly. JavaScript's `JSON.parse` does
  not: above 2^53 it silently rounds. Use the Node SDK, which parses losslessly.
- `POST …/next?count=N` returns `{ first, last, count, step }`; without `count` it
  returns `{ id }`. Send `Idempotency-Key: <unique value>` to make a retry return the
  original IDs instead of issuing new ones (remembered for 24 hours).
- Lists are paged: pass `meta.next_cursor` back as `?cursor=` until it is `null`.
  Optional `limit` (1-500, default 50) and `prefix` (only names starting with it).

## Command-line client (`idbseq`)

The `idbseq` binary ships in every release archive and in the Docker image. The
server URL and credential come from `--url` / `--key`, else `IDBSEQ_URL` /
`IDBSEQ_KEY`, else `~/.config/idbseq/config.toml`; after `idbseq login` the
stored admin session is used unless a key is given.

```bash
export IDBSEQ_URL=http://127.0.0.1:7878 IDBSEQ_KEY=idb_...
idbseq next orders             # 1044
idbseq next orders --count 100 # 1045..1144
idbseq --json collections info orders
```

**`next` takes IDs; `last` and `info` only look.** Each `next` moves the counter
forward and hands you the ID for good: no other client will ever get it, even if
you never use it, so the next call returns the following one. `last` and
`collections info` show `next_id` without taking it. That value is only a
preview: it is not reserved, and another client may take it first, so always
use `next` for an ID you keep. If a `next` call fails and you retry it, pass the
same `--idempotency-key` to get the original IDs back instead of using up new
ones. The MCP tools work the same way: `next_id` takes IDs, `get_collection` and
`list_collections` only read.

| Command                                                                                                               | Does                                                                       |
| --------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `idbseq next NAME [--count N] [--idempotency-key K]`                                                                  | Take the next ID, or N at once; a repeated key returns the same IDs        |
| `idbseq last NAME`                                                                                                    | Show the last issued ID and the next one, without using anything           |
| `idbseq collections list [--limit N] [--cursor C] [--prefix P] [--all]`                                               | List collections; `--all` follows every page                               |
| `idbseq collections create NAME [--start] [--step] [--max] [--durability strict\|block --block-size N] [--protected]` | Create a collection; `--protected` needs the admin                         |
| `idbseq collections info NAME`                                                                                        | Show a collection's settings and its last and next ID                      |
| `idbseq collections update NAME [--start] [--step] [--max] [--durability …] [--protected true\|false]`                | Change a collection's settings                                             |
| `idbseq collections delete NAME [--yes]`                                                                              | Delete a collection and its counter (asks first)                           |
| `idbseq reset NAME [--value N] [--allow-rewind] [--yes]`                                                              | Reset the counter (asks first); moving it backwards needs `--allow-rewind` |
| `idbseq keys create NAME --scope S[,S] [--collection C[,C]]`                                                          | Create an API key (admin); the secret is shown once                        |
| `idbseq keys list`                                                                                                    | List API keys, without secrets (admin)                                     |
| `idbseq keys revoke ID [--yes]`                                                                                       | Revoke an API key (admin; asks first)                                      |
| `idbseq login [--username admin]`                                                                                     | Log in as the admin and store the 15-minute session                        |
| `idbseq logout`                                                                                                       | Revoke the stored session and remove it                                    |
| `idbseq audit [--limit] [--cursor] [--collection] [--action]`                                                         | Show the audit log, newest first (admin)                                   |

Every command also takes `--url`, `--key`, `--json` (print the response `data`
as JSON; errors go to stderr) and `--timeout SECONDS` (default 30). `delete`,
`reset` and `keys revoke` ask you to type the name or key id; `--yes` skips that.

Exit codes, so scripts can tell failures apart (`idbseq --help` prints them too):

| Code | Meaning                                                                  |
| ---- | ------------------------------------------------------------------------ |
| 0    | success                                                                  |
| 1    | other errors: `INTERNAL`, unknown codes, unexpected responses, local I/O |
| 2    | invalid arguments or configuration                                       |
| 3    | network error: server unreachable or timed out                           |
| 4    | `UNAUTHORIZED`                                                           |
| 5    | `FORBIDDEN`                                                              |
| 6    | `PROTECTED_COLLECTION`                                                   |
| 7    | `COLLECTION_NOT_FOUND`, `NOT_FOUND`                                      |
| 8    | `COLLECTION_EXISTS`                                                      |
| 9    | `SEQUENCE_EXHAUSTED`                                                     |
| 10   | `VALIDATION_FAILED`, `PAYLOAD_TOO_LARGE`                                 |
| 11   | `RATE_LIMITED`                                                           |
| 12   | aborted: confirmation declined                                           |

The API call behind each command and the config file format are in the
[CLI README](../crates/idbseq-cli/README.md). Server commands (`idbseqd`) are in
[operations.md](operations.md#commands).

## curl

```bash
KEY="$IDBSEQ_KEY"
URL="${IDBSEQ_URL:-http://127.0.0.1:7878}"

# Generate one ID  -> {"success":true,"data":{"id":1044},"error":null}
curl -sS -X POST -H "Authorization: Bearer $KEY" -H 'Content-Length: 0' \
  "$URL/v1/collections/examples/next"

# Generate a batch, safe to retry
# -> data: {"first":1045,"last":1047,"count":3,"step":1}
curl -sS -X POST -H "Authorization: Bearer $KEY" -H 'Content-Length: 0' \
  -H "Idempotency-Key: order-batch-7f3a" \
  "$URL/v1/collections/examples/next?count=3"

# Last ID -> data: {"last_id":1047,"next_id":1048}
curl -sS -H "Authorization: Bearer $KEY" "$URL/v1/collections/examples/last"

# List (one page; repeat with &cursor=<meta.next_cursor> until it is null)
curl -sS -G -H "Authorization: Bearer $KEY" \
  --data-urlencode limit=100 --data-urlencode prefix=ex "$URL/v1/collections"
```

`curl -f` hides the error body, so the script captures the status with `-w` instead
and still prints the envelope's code and message
([full script](../examples/curl/idbseq.sh)):

```bash
api() {
  local method="$1" path="$2" out status body code message
  shift 2
  if ! out="$(curl -sS --max-time 10 -X "$method" \
    -H "Authorization: Bearer ${IDBSEQ_KEY}" \
    -H 'Accept: application/json' \
    -w '\n%{http_code}' "$@" "${IDBSEQ_URL}${path}")"; then
    echo "error: NETWORK_ERROR: ${method} ${IDBSEQ_URL}${path} failed" >&2
    return 1
  fi
  status="${out##*$'\n'}"
  body="${out%$'\n'*}"
  if [[ "$status" != 2?? ]]; then
    code="$(json_str "$body" code)"
    message="$(json_str "$body" message)"
    echo "error: ${code:-HTTP_${status}} (HTTP ${status}):" \
      "${message:-$body}" >&2
    return 1
  fi
  printf '%s\n' "$body"
}
```

## Python

Standard library only; `json.loads` keeps 64-bit IDs exact
([full program](../examples/python/idbseq_example.py)).

```python
class ApiError(Exception):
    """A failed call: HTTP status (0 for network errors), error code, message."""

    def __init__(self, status, code, message):
        super().__init__(f"{code} (HTTP {status}): {message}")
        self.status = status
        self.code = code
        self.message = message


class Client:
    def __init__(self, base_url, key):
        self.base_url = base_url.rstrip("/")
        self.key = key

    def call(self, method, path, query=None, headers=None):
        """Send a request and return the success envelope, or raise ApiError."""
        url = self.base_url + path
        if query:
            url += "?" + urllib.parse.urlencode(query)
        request = urllib.request.Request(
            url,
            method=method,
            data=b"" if method == "POST" else None,
            headers={
                "Authorization": f"Bearer {self.key}",
                "Accept": "application/json",
                **(headers or {}),
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
                status, raw = response.status, response.read()
        except urllib.error.HTTPError as err:  # non-2xx: the body is the error envelope
            status, raw = err.code, err.read()
        except (OSError, http.client.HTTPException) as err:  # URLError, timeouts, resets
            raise ApiError(0, "NETWORK_ERROR", str(getattr(err, "reason", err))) from err

        try:
            envelope = json.loads(raw)
        except ValueError:
            envelope = None
        if not isinstance(envelope, dict):
            raise ApiError(status, f"HTTP_{status}", raw.decode("utf-8", "replace")[:200])
        if 200 <= status < 300 and envelope.get("success") is True:
            return envelope
        error = envelope.get("error") or {}
        # Unknown or missing codes are still errors of this HTTP status.
        raise ApiError(status, error.get("code") or f"HTTP_{status}", error.get("message") or "")

    def generate(self, collection):
        path = f"/v1/collections/{urllib.parse.quote(collection, safe='')}/next"
        return self.call("POST", path)["data"]["id"]

    def generate_batch(self, collection, count, idempotency_key=None):
        path = f"/v1/collections/{urllib.parse.quote(collection, safe='')}/next"
        headers = {"Idempotency-Key": idempotency_key} if idempotency_key else None
        return self.call("POST", path, {"count": count}, headers)["data"]

    def last(self, collection):
        path = f"/v1/collections/{urllib.parse.quote(collection, safe='')}/last"
        return self.call("GET", path)["data"]

    def list_all(self, prefix=None):
        """Yield every collection, following meta.next_cursor."""
        cursor = None
        while True:
            query = {"limit": PAGE_LIMIT}
            if prefix:
                query["prefix"] = prefix
            if cursor:
                query["cursor"] = cursor
            envelope = self.call("GET", "/v1/collections", query)
            yield from envelope["data"]
            cursor = (envelope.get("meta") or {}).get("next_cursor")
            if not cursor:
                return
```

Usage:

```python
client = Client(os.environ.get("IDBSEQ_URL") or DEFAULT_URL, key)
try:
    print(client.generate("examples"))                       # 1044
    print(client.generate_batch("examples", 3, f"example-{uuid.uuid4()}"))
    print(client.last("examples"))                           # {'last_id': 1047, 'next_id': 1048}
    for item in client.list_all():
        print(item["name"], item["last_id"])
except ApiError as err:
    print(f"error: {err}", file=sys.stderr)                  # error: COLLECTION_NOT_FOUND (HTTP 404): ...
    sys.exit(1)
```

## Go

Standard library only ([full program](../examples/go/main.go), with `go.mod`).
Decode IDs into `int64` fields (`*int64` where they may be `null`). If you decode
into `map[string]any` instead, use `json.Decoder.UseNumber()`: the default turns
numbers into `float64` and loses precision above 2^53.

```go
type envelope struct {
	Success bool            `json:"success"`
	Data    json.RawMessage `json:"data"`
	Error   *apiError       `json:"error"`
	Meta    *meta           `json:"meta"`
}

// call sends a request and returns the success envelope, or a *requestError.
func (c *client) call(method, path string, query url.Values, headers map[string]string) (*envelope, error) {
	target := c.baseURL + path
	if len(query) > 0 {
		target += "?" + query.Encode()
	}
	req, err := http.NewRequest(method, target, nil)
	if err != nil {
		return nil, &requestError{Code: "BAD_REQUEST", Message: err.Error()}
	}
	req.Header.Set("Authorization", "Bearer "+c.key)
	req.Header.Set("Accept", "application/json")
	for name, value := range headers {
		req.Header.Set(name, value)
	}
	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, &requestError{Code: "NETWORK_ERROR", Message: err.Error()}
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, &requestError{Status: resp.StatusCode, Code: "NETWORK_ERROR", Message: err.Error()}
	}

	var env envelope
	decodeErr := json.Unmarshal(raw, &env)
	if decodeErr == nil && env.Success && resp.StatusCode/100 == 2 {
		return &env, nil
	}
	// Unknown or missing codes are still errors of this HTTP status.
	failure := &requestError{Status: resp.StatusCode, Code: "HTTP_" + strconv.Itoa(resp.StatusCode), Message: string(raw)}
	if decodeErr == nil && env.Error != nil {
		if env.Error.Code != "" {
			failure.Code = env.Error.Code
		}
		failure.Message = env.Error.Message
	}
	return nil, failure
}
```

Generate, batch with an idempotency key, and list with the cursor:

```go
func (c *client) generate(name string) (int64, error) {
	var out nextID
	err := c.data(http.MethodPost, collectionPath(name, "/next"), nil, nil, &out)
	return out.ID, err
}

func (c *client) generateBatch(name string, count int, idempotencyKey string) (batch, error) {
	var out batch
	query := url.Values{"count": {strconv.Itoa(count)}}
	headers := map[string]string{"Idempotency-Key": idempotencyKey}
	err := c.data(http.MethodPost, collectionPath(name, "/next"), query, headers, &out)
	return out, err
}

// listAll returns every collection, following meta.next_cursor.
func (c *client) listAll() ([]collection, error) {
	var all []collection
	cursor := ""
	for {
		query := url.Values{"limit": {strconv.Itoa(pageLimit)}}
		if cursor != "" {
			query.Set("cursor", cursor)
		}
		env, err := c.call(http.MethodGet, "/v1/collections", query, nil)
		if err != nil {
			return nil, err
		}
		var page []collection
		if err := json.Unmarshal(env.Data, &page); err != nil {
			return nil, &requestError{Status: 200, Code: "DECODE_ERROR", Message: err.Error()}
		}
		all = append(all, page...)
		if env.Meta == nil || env.Meta.NextCursor == nil {
			return all, nil
		}
		cursor = *env.Meta.NextCursor
	}
}
```

## Java

Java 17+, JDK only, a single file run with `java IdbSeqExample.java`
([full program](../examples/java/IdbSeqExample.java)). The JDK has no JSON parser,
so the file carries a minimal one that reads integers as `long`; in a real project
use Jackson or Gson and map IDs to `long`.

```java
Map<String, Object> call(
    String method,
    String pathAndQuery,
    Map<String, String> headers
) throws ApiException {
    HttpRequest.Builder builder = HttpRequest.newBuilder(
        URI.create(baseUrl + pathAndQuery)
    )
        .timeout(TIMEOUT)
        .header("Authorization", "Bearer " + key)
        .header("Accept", "application/json")
        .method(method, HttpRequest.BodyPublishers.noBody());
    headers.forEach(builder::header);

    HttpResponse<String> response;
    try {
        response = http.send(
            builder.build(),
            HttpResponse.BodyHandlers.ofString()
        );
    } catch (IOException e) {
        throw new ApiException(0, "NETWORK_ERROR", e.toString());
    } catch (InterruptedException e) {
        Thread.currentThread().interrupt();
        throw new ApiException(0, "INTERRUPTED", e.toString());
    }

    int status = response.statusCode();
    Map<String, Object> envelope;
    try {
        envelope = asMap(Json.parse(response.body()));
    } catch (IllegalArgumentException e) {
        envelope = null;
    }
    if (envelope == null) {
        throw new ApiException(status, "HTTP_" + status, response.body());
    }
    if (status / 100 == 2 && Boolean.TRUE.equals(envelope.get("success"))) {
        return envelope;
    }
    // Unknown or missing codes are still errors of this HTTP status.
    Map<String, Object> error = asMap(envelope.get("error"));
    String code =
        error != null && error.get("code") instanceof String c
            ? c
            : "HTTP_" + status;
    String message =
        error != null && error.get("message") instanceof String m ? m : "";
    throw new ApiException(status, code, message);
}

long generate(String name) throws ApiException {
    return (Long) data("POST", collectionPath(name, "/next"), Map.of()).get(
        "id"
    );
}

Map<String, Object> generateBatch(
    String name,
    int count,
    String idempotencyKey
) throws ApiException {
    String path = collectionPath(name, "/next") + "?count=" + count;
    return data("POST", path, Map.of("Idempotency-Key", idempotencyKey));
}

/** Returns every collection, following meta.next_cursor. */
List<Map<String, Object>> listAll() throws ApiException {
    List<Map<String, Object>> all = new ArrayList<>();
    String cursor = null;
    do {
        String query =
            "?limit=" +
            PAGE_LIMIT +
            (cursor == null ? "" : "&cursor=" + encode(cursor));
        Map<String, Object> envelope = call(
            "GET",
            "/v1/collections" + query,
            Map.of()
        );
        if (!(envelope.get("data") instanceof List<?> page)) {
            throw new ApiException(
                200,
                "DECODE_ERROR",
                "expected an array in `data`"
            );
        }
        for (Object item : page) {
            all.add(asMap(item));
        }
        Map<String, Object> meta = asMap(envelope.get("meta"));
        cursor =
            meta != null && meta.get("next_cursor") instanceof String next
                ? next
                : null;
    } while (cursor != null);
    return all;
}
```

## AI agents (MCP server)

[`sdk/mcp`](../sdk/mcp) (npm package `idbseq-mcp`) is a stdio
[MCP](https://modelcontextprotocol.io) server built on the Node SDK. An agent in
Claude Code, Claude Desktop or any other MCP client takes IDs with one tool call,
and several agents at once never collide:

```bash
idbseq keys create my-agent --scope generate --scope read --collection tickets
claude mcp add idbseq \
  --env IDBSEQ_URL=http://127.0.0.1:7878 \
  --env IDBSEQ_KEY=idb_... \
  -- npx -y idbseq-mcp
```

It offers `next_id` (one ID or a batch, with an optional idempotency key),
`list_collections` and `get_collection`; `create_collection` only with
`--allow-create`; never reset or delete. Like every client it needs an API key:
give it `generate` and `read` and a collection allow-list. Setup for other
clients is in its [README](../sdk/mcp/README.md).

## Generating a client from OpenAPI

The contract is [`api/openapi.json`](../api/openapi.json) in this repository, and a
running server serves the same document at `GET /v1/openapi.json` (no auth):

```bash
curl -sS -o openapi.json http://127.0.0.1:7878/v1/openapi.json
```

Generate with [openapi-generator](https://openapi-generator.tech), either the
Docker image or the npm wrapper (which downloads the jar; needs a JRE). Pin the
generator version so regenerating is reproducible.

```bash
# Docker: mount the repo at /local
gen() {
  docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:/local" \
    openapitools/openapi-generator-cli:v7.10.0 "$@"
}

gen generate -i /local/api/openapi.json -g python -o /local/clients/python \
  --additional-properties=packageName=idbseq_client
gen generate -i /local/api/openapi.json -g go -o /local/clients/go \
  --additional-properties=packageName=idbseq
gen generate -i /local/api/openapi.json -g java -o /local/clients/java \
  --library native \
  --additional-properties=groupId=com.example,artifactId=idbseq-client \
  --additional-properties=invokerPackage=com.example.idbseq
gen generate -i /local/api/openapi.json -g typescript-fetch \
  -o /local/clients/ts --additional-properties=supportsES6=true

# npm wrapper: same arguments, local paths
npx @openapitools/openapi-generator-cli version-manager set 7.10.0
npx @openapitools/openapi-generator-cli generate \
  -i api/openapi.json -g typescript-fetch -o clients/ts
```

Things to know about generated clients:

- **The envelope is part of every response model.** A generated call returns the
  whole `{ success, data, error }` object, so read `.data` (for example
  `response.data.id`). Non-2xx responses raise the generator's exception
  (`ApiException` in Python and Java, `GenericOpenAPIError` in Go, `ResponseError`
  in typescript-fetch); its body is the error envelope, so parse it for
  `error.code` and fall back to the HTTP status for codes you do not know.
- **Bearer auth** is a `bearerAuth` security scheme. Set the key once:
  - Python: `idbseq_client.Configuration(host=url, access_token=key)`
  - Go: `ctx := context.WithValue(ctx, idbseq.ContextAccessToken, key)` and pass `ctx` to each call
  - Java (`native`): `apiClient.setRequestInterceptor(b -> b.header("Authorization", "Bearer " + key))`
  - typescript-fetch: `new Configuration({ basePath: url, accessToken: key })`
- **int64 in JavaScript.** IDs are `int64`; typescript-fetch (and other JS
  generators) map them to `number` and parse with `response.json()`, which rounds
  values above 2^53 (9007199254740991) without an error. Either keep every
  collection's `max` at or below that, or use the Node SDK
  (`idbseq`), which parses IDs losslessly. Python, Go and Java
  generators map `int64` to exact types.
