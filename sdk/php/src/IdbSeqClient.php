<?php

declare(strict_types=1);

namespace IdbSeq;

use IdbSeq\Exception\IdbSeqException;
use IdbSeq\Exception\UnexpectedResponseException;
use IdbSeq\Internal\ApiResponse;
use IdbSeq\Internal\Decoder;
use IdbSeq\Internal\Transport;
use IdbSeq\Internal\Uuid;
use IdbSeq\Model\ApiKey;
use IdbSeq\Model\Collection;
use IdbSeq\Model\CollectionPage;
use IdbSeq\Model\Durability;
use IdbSeq\Model\IdBatch;
use IdbSeq\Model\LastId;
use IdbSeq\Model\ResetResult;
use IdbSeq\Model\Session;
use Closure;
use Http\Discovery\Psr17FactoryDiscovery;
use Http\Discovery\Psr18ClientDiscovery;
use InvalidArgumentException;
use LogicException;
use Psr\Http\Client\ClientInterface;
use Psr\Http\Message\RequestFactoryInterface;
use Psr\Http\Message\StreamFactoryInterface;
use UnexpectedValueException;

/**
 * Client for the idbseq HTTP API v1.
 *
 * Every method throws an {@see IdbSeqException} subclass on failure.
 */
final class IdbSeqClient
{
    public const DEFAULT_URL = 'http://127.0.0.1:7878';

    private readonly Transport $transport;
    private readonly RetryPolicy $retry;
    private ?string $sessionToken = null;

    /**
     * @param string $url server base URL
     * @param string|null $apiKey API key `idb_<id>_<secret>` (or any bearer token)
     * @param ClientInterface|null $httpClient PSR-18 client; found with php-http/discovery when null
     * @param RequestFactoryInterface|null $requestFactory PSR-17 factory; discovered when null
     * @param StreamFactoryInterface|null $streamFactory PSR-17 factory; discovered when null
     * @param RetryPolicy|null $retry retries for generateId()/generateIds(); RetryPolicy::none() disables
     */
    public function __construct(
        string $url = self::DEFAULT_URL,
        private readonly ?string $apiKey = null,
        ?ClientInterface $httpClient = null,
        ?RequestFactoryInterface $requestFactory = null,
        ?StreamFactoryInterface $streamFactory = null,
        ?RetryPolicy $retry = null,
    ) {
        $this->transport = new Transport(
            self::normaliseUrl($url),
            $httpClient ?? self::discover(static fn () => Psr18ClientDiscovery::find()),
            $requestFactory ?? self::discover(static fn () => Psr17FactoryDiscovery::findRequestFactory()),
            $streamFactory ?? self::discover(static fn () => Psr17FactoryDiscovery::findStreamFactory()),
        );
        $this->retry = $retry ?? new RetryPolicy();
    }

    // IDs

    /** Consumes and returns the next ID. Retried safely with an idempotency key. */
    public function generateId(string $name, ?string $idempotencyKey = null): int
    {
        $response = $this->generate($name, [], $idempotencyKey);

        return $this->decode($response, static fn (ApiResponse $r): int => Decoder::generatedId($r->data));
    }

    /** Consumes `$count` IDs at once; the whole batch is issued or nothing is. */
    public function generateIds(string $name, int $count, ?string $idempotencyKey = null): IdBatch
    {
        if ($count < 1) {
            throw new InvalidArgumentException('count must be >= 1');
        }
        $response = $this->generate($name, ['count' => $count], $idempotencyKey);

        return $this->decode($response, static fn (ApiResponse $r): IdBatch => Decoder::batch($r->data));
    }

    /** Reads the last issued ID and the next one without consuming anything. */
    public function lastId(string $name): LastId
    {
        $response = $this->send('GET', self::collectionPath($name, '/last'));

        return $this->decode($response, static fn (ApiResponse $r): LastId => Decoder::lastId($r->data));
    }

    // Collections

    public function listCollections(?int $limit = null, ?string $cursor = null, ?string $prefix = null): CollectionPage
    {
        $query = ['limit' => $limit, 'cursor' => $cursor, 'prefix' => $prefix];
        $response = $this->send('GET', 'v1/collections', $query);

        return $this->decode(
            $response,
            static fn (ApiResponse $r): CollectionPage => Decoder::collectionPage($r->data, $r->meta),
        );
    }

    public function getCollection(string $name): Collection
    {
        return $this->collection($this->send('GET', self::collectionPath($name)));
    }

    public function createCollection(
        string $name,
        ?int $start = null,
        ?int $step = null,
        ?int $max = null,
        ?bool $protected = null,
        ?Durability $durability = null,
    ): Collection {
        $body = self::collectionBody(compact('name', 'start', 'step', 'max', 'protected'), $durability);

        return $this->collection($this->send('POST', 'v1/collections', [], $body));
    }

    public function updateCollection(
        string $name,
        ?int $max = null,
        ?bool $protected = null,
        ?int $start = null,
        ?int $step = null,
        ?Durability $durability = null,
    ): Collection {
        $body = self::collectionBody(compact('max', 'protected', 'start', 'step'), $durability);

        return $this->collection($this->send('PATCH', self::collectionPath($name), [], $body));
    }

    /** Resets the counter; sends `confirm: $name` for you. */
    public function resetCollection(string $name, ?int $value = null, bool $allowRewind = false): ResetResult
    {
        $body = array_filter(
            ['confirm' => $name, 'value' => $value, 'allow_rewind' => $allowRewind ?: null],
            static fn ($field): bool => $field !== null,
        );
        $response = $this->send('POST', self::collectionPath($name, '/reset'), [], $body);

        return $this->decode($response, static fn (ApiResponse $r): ResetResult => Decoder::resetResult($r->data));
    }

    /** Deletes the collection; sends `confirm` for you. Returns it as it was. */
    public function deleteCollection(string $name): Collection
    {
        return $this->collection($this->send('DELETE', self::collectionPath($name), ['confirm' => $name]));
    }

    // Admin

    /** Logs in as the admin; later calls use the session token instead of the API key. */
    public function login(string $username, string $password): Session
    {
        $body = ['username' => $username, 'password' => $password];
        $response = $this->transport->send('POST', 'v1/auth/login', null, [], $body);
        $session = $this->decode($response, static fn (ApiResponse $r): Session => Decoder::session($r->data));
        $this->sessionToken = $session->token;

        return $session;
    }

    /** Revokes the admin session, if any, and falls back to the API key. */
    public function logout(): void
    {
        if ($this->sessionToken === null) {
            return;
        }
        $this->send('POST', 'v1/auth/logout');
        $this->sessionToken = null;
    }

    /**
     * @param list<'read'|'generate'|'manage'> $scopes
     * @param list<string>|null $collections allow-list of names or `prefix*` patterns
     *
     * @return ApiKey with `key` set; the full key is shown only here
     */
    public function createKey(string $name, array $scopes, ?array $collections = null): ApiKey
    {
        $body = array_filter(compact('name', 'scopes', 'collections'), static fn ($v): bool => $v !== null);
        $response = $this->send('POST', 'v1/keys', [], $body);

        return $this->decode($response, static fn (ApiResponse $r): ApiKey => Decoder::apiKey($r->data));
    }

    /** @return list<ApiKey> */
    public function listKeys(): array
    {
        $response = $this->send('GET', 'v1/keys');

        return $this->decode($response, static fn (ApiResponse $r): array => Decoder::apiKeys($r->data));
    }

    public function revokeKey(string $id): void
    {
        if ($id === '') {
            throw new InvalidArgumentException('key id must not be empty');
        }
        $this->send('DELETE', 'v1/keys/' . rawurlencode($id));
    }

    // Plumbing

    /**
     * @param array<string, string|int|null> $query
     * @param array<string, mixed>|null $body
     * @param array<string, string> $headers
     */
    private function send(
        string $method,
        string $path,
        array $query = [],
        ?array $body = null,
        array $headers = [],
    ): ApiResponse {
        return $this->transport->send($method, $path, $this->sessionToken ?? $this->apiKey, $query, $body, $headers);
    }

    /** @param array<string, int> $query */
    private function generate(string $name, array $query, ?string $idempotencyKey): ApiResponse
    {
        $path = self::collectionPath($name, '/next');
        $headers = ['Idempotency-Key' => $idempotencyKey ?? Uuid::v4()];

        return $this->retry->run(fn (): ApiResponse => $this->send('POST', $path, $query, null, $headers));
    }

    private function collection(ApiResponse $response): Collection
    {
        return $this->decode($response, static fn (ApiResponse $r): Collection => Decoder::collection($r->data));
    }

    /**
     * @template T
     * @param Closure(ApiResponse): T $decoder
     * @return T
     */
    private function decode(ApiResponse $response, Closure $decoder): mixed
    {
        try {
            return $decoder($response);
        } catch (UnexpectedValueException $e) {
            throw new UnexpectedResponseException(
                "unexpected response data: {$e->getMessage()}",
                $response->status,
                $response->requestId,
                $e,
            );
        }
    }

    /**
     * @param array<string, mixed> $fields
     * @return array<string, mixed>
     */
    private static function collectionBody(array $fields, ?Durability $durability): array
    {
        $body = array_filter($fields, static fn ($value): bool => $value !== null);

        return $durability === null ? $body : [...$body, 'durability' => $durability->toArray()];
    }

    private static function collectionPath(string $name, string $suffix = ''): string
    {
        if ($name === '') {
            throw new InvalidArgumentException('collection name must not be empty');
        }

        return 'v1/collections/' . rawurlencode($name) . $suffix;
    }

    private static function normaliseUrl(string $url): string
    {
        $scheme = parse_url($url, PHP_URL_SCHEME);
        $host = parse_url($url, PHP_URL_HOST);
        if (!in_array($scheme, ['http', 'https'], true) || !is_string($host) || $host === '') {
            throw new InvalidArgumentException("invalid idbseq URL: {$url}");
        }

        return rtrim($url, '/') . '/';
    }

    /**
     * @template T of object
     * @param Closure(): T $find
     * @return T
     */
    private static function discover(Closure $find): object
    {
        if (!class_exists(Psr18ClientDiscovery::class)) {
            throw new LogicException(
                'Pass a PSR-18 client and PSR-17 factories, or install php-http/discovery to find them.',
            );
        }

        return $find();
    }
}
