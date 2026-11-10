<?php

declare(strict_types=1);

namespace IdbSeq\Internal;

use IdbSeq\Exception\ErrorFactory;
use IdbSeq\Exception\IdbSeqException;
use IdbSeq\Exception\NetworkException;
use IdbSeq\Exception\UnexpectedResponseException;
use JsonException;
use Psr\Http\Client\ClientExceptionInterface;
use Psr\Http\Client\ClientInterface;
use Psr\Http\Message\RequestFactoryInterface;
use Psr\Http\Message\RequestInterface;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\StreamFactoryInterface;

/** @internal Builds PSR-7 requests, sends them and unwraps the envelope. */
final class Transport
{
    private const USER_AGENT = 'idbseq-php/0.1.0';
    private const JSON_DEPTH = 512;

    public function __construct(
        private readonly string $baseUrl,
        private readonly ClientInterface $httpClient,
        private readonly RequestFactoryInterface $requestFactory,
        private readonly StreamFactoryInterface $streamFactory,
    ) {
    }

    /**
     * @param array<string, string|int|null> $query
     * @param array<string, mixed>|null $body
     * @param array<string, string> $headers
     *
     * @throws IdbSeqException
     */
    public function send(
        string $method,
        string $path,
        ?string $credential,
        array $query = [],
        ?array $body = null,
        array $headers = [],
    ): ApiResponse {
        $request = $this->buildRequest($method, $this->url($path, $query), $credential, $body, $headers);
        try {
            $response = $this->httpClient->sendRequest($request);
        } catch (ClientExceptionInterface $e) {
            throw new NetworkException("request failed: {$e->getMessage()}", $e);
        }

        return $this->interpret($response);
    }

    /** @param array<string, string|int|null> $query */
    private function url(string $path, array $query): string
    {
        $present = array_filter($query, static fn ($value): bool => $value !== null);
        $queryString = http_build_query($present, '', '&', PHP_QUERY_RFC3986);

        return $this->baseUrl . $path . ($queryString === '' ? '' : "?{$queryString}");
    }

    /**
     * @param array<string, mixed>|null $body
     * @param array<string, string> $headers
     */
    private function buildRequest(
        string $method,
        string $url,
        ?string $credential,
        ?array $body,
        array $headers,
    ): RequestInterface {
        $request = $this->requestFactory->createRequest($method, $url)
            ->withHeader('Accept', 'application/json')
            ->withHeader('User-Agent', self::USER_AGENT);
        if ($credential !== null) {
            $request = $request->withHeader('Authorization', "Bearer {$credential}");
        }
        foreach ($headers as $name => $value) {
            $request = $request->withHeader($name, $value);
        }
        if ($body === null) {
            return $request;
        }

        return $request
            ->withHeader('Content-Type', 'application/json')
            ->withBody($this->streamFactory->createStream(self::encode($body)));
    }

    /** @param array<string, mixed> $body */
    private static function encode(array $body): string
    {
        if ($body === []) {
            return '{}';
        }

        return json_encode($body, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    }

    private function interpret(ResponseInterface $response): ApiResponse
    {
        $status = $response->getStatusCode();
        $requestId = $response->hasHeader('X-Request-Id') ? $response->getHeaderLine('X-Request-Id') : null;
        $body = $this->parseBody((string) $response->getBody(), $status, $requestId);
        if ($body['success'] === false) {
            $retryAfter = RetryAfter::parse($response->getHeaderLine('Retry-After'), time());
            throw $this->errorFromEnvelope($body['error'] ?? null, $status, $requestId, $retryAfter);
        }
        if ($status >= 400) {
            throw new UnexpectedResponseException("HTTP {$status} with a success envelope", $status, $requestId);
        }

        return new ApiResponse($body['data'] ?? null, $body['meta'] ?? null, $status, $requestId);
    }

    /** @return array<string, mixed> */
    private function parseBody(string $text, int $status, ?string $requestId): array
    {
        try {
            $body = json_decode($text, true, self::JSON_DEPTH, JSON_THROW_ON_ERROR | JSON_BIGINT_AS_STRING);
        } catch (JsonException $e) {
            throw new UnexpectedResponseException("HTTP {$status} with a non-JSON body", $status, $requestId, $e);
        }
        if (!is_array($body) || !is_bool($body['success'] ?? null)) {
            throw new UnexpectedResponseException("HTTP {$status} without an idbseq envelope", $status, $requestId);
        }

        /** @var array<string, mixed> $body */
        return $body;
    }

    private function errorFromEnvelope(mixed $error, int $status, ?string $requestId, ?int $retryAfter): IdbSeqException
    {
        if (!is_array($error) || !is_string($error['code'] ?? null)) {
            return new UnexpectedResponseException(
                "HTTP {$status} with an error envelope that has no error code",
                $status,
                $requestId,
            );
        }
        $code = $error['code'];
        $message = is_string($error['message'] ?? null) ? $error['message'] : $code;

        return ErrorFactory::fromCode($code, $message, $status, $error['details'] ?? null, $requestId, $retryAfter);
    }
}
