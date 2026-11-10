<?php

declare(strict_types=1);

namespace IdbSeq\Tests\Support;

use LogicException;
use Nyholm\Psr7\Response;
use Psr\Http\Client\ClientExceptionInterface;
use Psr\Http\Client\ClientInterface;
use Psr\Http\Message\RequestInterface;
use Psr\Http\Message\ResponseInterface;
use RuntimeException;

/** A PSR-18 client that answers with queued responses and records every request. */
final class MockHttpClient implements ClientInterface
{
    /** @var list<RequestInterface> */
    public array $requests = [];

    /** @var list<ResponseInterface|ClientExceptionInterface> */
    private array $queue = [];

    public function queue(ResponseInterface|ClientExceptionInterface ...$replies): void
    {
        $this->queue = [...$this->queue, ...array_values($replies)];
    }

    public function sendRequest(RequestInterface $request): ResponseInterface
    {
        $this->requests[] = $request;
        $reply = array_shift($this->queue) ?? throw new LogicException('the test queued no reply');
        if ($reply instanceof ClientExceptionInterface) {
            throw $reply;
        }

        return $reply;
    }

    public function lastRequest(): RequestInterface
    {
        return $this->requests[array_key_last($this->requests) ?? throw new LogicException('no request')];
    }

    /** @param array<string, string> $headers */
    public static function ok(mixed $data, mixed $meta = null, int $status = 200, array $headers = []): ResponseInterface
    {
        $envelope = ['success' => true, 'data' => $data, 'error' => null];
        if ($meta !== null) {
            $envelope['meta'] = $meta;
        }

        return self::json($status, $envelope, $headers);
    }

    /** @param array<string, string> $headers */
    public static function fail(
        int $status,
        string $code,
        string $message = 'failed',
        mixed $details = null,
        array $headers = [],
    ): ResponseInterface {
        $envelope = ['success' => false, 'data' => null, 'error' => compact('code', 'message', 'details')];

        return self::json($status, $envelope, $headers);
    }

    /** @param array<string, string> $headers */
    public static function raw(int $status, string $body, array $headers = []): ResponseInterface
    {
        return new Response($status, ['Content-Type' => 'application/json', 'X-Request-Id' => 'req-test-1', ...$headers], $body);
    }

    public static function networkError(string $message = 'connection refused'): ClientExceptionInterface
    {
        return new class ($message) extends RuntimeException implements ClientExceptionInterface {
        };
    }

    /**
     * @param array<string, mixed> $envelope
     * @param array<string, string> $headers
     */
    private static function json(int $status, array $envelope, array $headers): ResponseInterface
    {
        return self::raw($status, json_encode($envelope, JSON_THROW_ON_ERROR), $headers);
    }
}
