<?php

declare(strict_types=1);

namespace IdbSeq\Tests\Support;

use IdbSeq\IdbSeqClient;
use IdbSeq\RetryPolicy;
use Nyholm\Psr7\Factory\Psr17Factory;
use PHPUnit\Framework\TestCase;
use Psr\Http\Message\RequestInterface;

abstract class ClientTestCase extends TestCase
{
    protected MockHttpClient $http;

    /** @var list<int> milliseconds the retry policy slept */
    protected array $sleeps = [];

    protected function setUp(): void
    {
        $this->http = new MockHttpClient();
        $this->sleeps = [];
    }

    protected function client(?string $apiKey = 'idb_k1_secret', ?RetryPolicy $retry = null): IdbSeqClient
    {
        $factory = new Psr17Factory();

        return new IdbSeqClient(
            url: 'http://ids.test:7878',
            apiKey: $apiKey,
            httpClient: $this->http,
            requestFactory: $factory,
            streamFactory: $factory,
            retry: $retry ?? $this->recordingRetry(),
        );
    }

    protected function recordingRetry(int $maxAttempts = 4, int $maxRetryAfterMs = 30000): RetryPolicy
    {
        return new RetryPolicy(
            maxAttempts: $maxAttempts,
            baseDelayMs: 10,
            maxDelayMs: 100,
            maxRetryAfterMs: $maxRetryAfterMs,
            sleeper: function (int $ms): void {
                $this->sleeps[] = $ms;
            },
        );
    }

    protected function lastRequest(): RequestInterface
    {
        return $this->http->lastRequest();
    }

    protected function lastBody(): string
    {
        return (string) $this->lastRequest()->getBody();
    }

    /** @return array<string, string> */
    protected function lastQuery(): array
    {
        parse_str($this->lastRequest()->getUri()->getQuery(), $query);

        /** @var array<string, string> $query */
        return $query;
    }
}
