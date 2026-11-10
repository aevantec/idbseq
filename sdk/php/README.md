# idbseq/idbseq

PHP client for the [idbseq](../../README.md) ID sequence server. PHP 8.1+ on a
64-bit build (IDs are PHP `int`). Uses any PSR-18 HTTP client with PSR-17
factories; there is no hard Guzzle dependency.

```php
use IdbSeq\IdbSeqClient;
use IdbSeq\Exception\SequenceExhaustedException;

// Pass your PSR-18 client and PSR-17 factories, e.g. Guzzle + nyholm/psr7 or Symfony HttpClient ...
$ids = new IdbSeqClient(
    url: 'http://127.0.0.1:7878',
    apiKey: getenv('IDBSEQ_KEY') ?: null,
    httpClient: $psr18Client,
    requestFactory: $psr17Factory,
    streamFactory: $psr17Factory,
);
// ... or install php-http/discovery and omit them.

$id = $ids->generateId('orders');                 // 1043
$batch = $ids->generateIds('orders', 100);        // IdBatch: first, last, count, step
$last = $ids->lastId('orders');                   // LastId: lastId, nextId
$page = $ids->listCollections(limit: 50);         // CollectionPage: items, total, limit, nextCursor

try {
    $ids->generateId('tickets');
} catch (SequenceExhaustedException $e) {
    // $e->errorCode(), $e->status(), $e->details(), $e->requestId()
}
```

## API

Same method set as the Node SDK, with named arguments instead of option objects:
`generateId($name, $idempotencyKey = null)`, `generateIds($name, $count, $idempotencyKey = null)`,
`lastId($name)`, `listCollections(limit:, cursor:, prefix:)`, `getCollection($name)`,
`createCollection($name, start:, step:, max:, protected:, durability:)`,
`updateCollection($name, max:, protected:, start:, step:, durability:)`,
`resetCollection($name, value:, allowRewind:)`, `deleteCollection($name)`,
`login($username, $password)`, `logout()`, `createKey($name, $scopes, $collections = null)`,
`listKeys()`, `revokeKey($id)`.

- `resetCollection` and `deleteCollection` send `confirm` for you. `login` stores
  the admin session token on the client until `logout()`.
- **Exceptions**: all extend `Exception\IdbSeqException` (`errorCode()`, `status()`,
  `details()`, `requestId()`, `retryAfterSeconds()`; `getCode()` is the HTTP
  status). One class per API code (`ValidationFailedException`,
  `UnauthorizedException`, `ForbiddenException`, `ProtectedCollectionException`,
  `NotFoundException`, `CollectionNotFoundException`, `CollectionExistsException`,
  `SequenceExhaustedException`, `PayloadTooLargeException`, `RateLimitedException`,
  `InternalException`), plus `UnknownCodeException`, `UnexpectedResponseException`
  and `NetworkException` (status 0).
- **Retries**: `generateId` / `generateIds` send an `Idempotency-Key` (random UUID
  per call, or yours) and retry network errors, 5xx and 429 with the same key,
  honouring `Retry-After`. Tune with `retry: new RetryPolicy(maxAttempts: 4,
  baseDelayMs: 100, maxDelayMs: 2000, maxRetryAfterMs: 30000)` or disable with
  `RetryPolicy::none()`. Timeouts are configured on your PSR-18 client.

## Development

```sh
composer update
composer test           # PHPUnit unit tests with a mocked PSR-18 client
composer phpstan        # level 8
composer test:coverage  # needs pcov or Xdebug; fails below 80% line coverage
IDBSEQ_E2E_URL=http://127.0.0.1:7878 IDBSEQ_E2E_ADMIN_PASSWORD=… composer test:e2e
```

The e2e suite runs the critical flow against a real `idbseqd` with the PSR-18
client found by php-http/discovery, and is skipped unless both variables are set.
