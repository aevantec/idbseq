<?php

declare(strict_types=1);

namespace IdbSeq\Tests\Unit;

use IdbSeq\Exception\InternalException;
use IdbSeq\Exception\NetworkException;
use IdbSeq\Exception\RateLimitedException;
use IdbSeq\Exception\SequenceExhaustedException;
use IdbSeq\Model\IdBatch;
use IdbSeq\Model\LastId;
use IdbSeq\RetryPolicy;
use IdbSeq\Tests\Support\ClientTestCase;
use IdbSeq\Tests\Support\MockHttpClient;
use InvalidArgumentException;

final class GenerateTest extends ClientTestCase
{
    private const UUID = '/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/';

    public function testGenerateIdPostsToNextWithKeyAndIdempotencyKey(): void
    {
        // Arrange
        $this->http->queue(MockHttpClient::ok(['id' => 1044]));

        // Act
        $id = $this->client()->generateId('orders');

        // Assert
        self::assertSame(1044, $id);
        $request = $this->lastRequest();
        self::assertSame('POST', $request->getMethod());
        self::assertSame('http://ids.test:7878/v1/collections/orders/next', (string) $request->getUri());
        self::assertSame('Bearer idb_k1_secret', $request->getHeaderLine('Authorization'));
        self::assertMatchesRegularExpression(self::UUID, $request->getHeaderLine('Idempotency-Key'));
    }

    public function testGenerateIdReturnsTheMaximumSignedSixtyFourBitId(): void
    {
        $this->http->queue(MockHttpClient::raw(200, '{"success":true,"data":{"id":9223372036854775807},"error":null}'));

        self::assertSame(PHP_INT_MAX, $this->client()->generateId('orders'));
    }

    public function testGenerateIdEncodesTheCollectionName(): void
    {
        $this->http->queue(MockHttpClient::ok(['id' => 1]));

        $this->client()->generateId('eu/orders');

        self::assertSame('/v1/collections/eu%2Forders/next', $this->lastRequest()->getUri()->getPath());
    }

    public function testGenerateIdUsesACallerSuppliedIdempotencyKey(): void
    {
        $this->http->queue(MockHttpClient::ok(['id' => 1]));

        $this->client()->generateId('orders', idempotencyKey: 'order-77');

        self::assertSame('order-77', $this->lastRequest()->getHeaderLine('Idempotency-Key'));
    }

    public function testGenerateIdRetriesServerAndNetworkErrorsWithTheSameKey(): void
    {
        // Arrange
        $this->http->queue(
            MockHttpClient::fail(500, 'INTERNAL'),
            MockHttpClient::networkError(),
            MockHttpClient::ok(['id' => 7]),
        );

        // Act
        $id = $this->client()->generateId('orders');

        // Assert
        self::assertSame(7, $id);
        self::assertCount(3, $this->http->requests);
        $keys = array_unique(array_map(
            static fn ($r): string => $r->getHeaderLine('Idempotency-Key'),
            $this->http->requests,
        ));
        self::assertCount(1, $keys);
        self::assertCount(2, $this->sleeps);
    }

    public function testEachLogicalCallGetsANewIdempotencyKey(): void
    {
        $this->http->queue(MockHttpClient::ok(['id' => 1]), MockHttpClient::ok(['id' => 2]));
        $client = $this->client();

        $client->generateId('orders');
        $client->generateId('orders');

        [$first, $second] = $this->http->requests;
        self::assertNotSame($first->getHeaderLine('Idempotency-Key'), $second->getHeaderLine('Idempotency-Key'));
    }

    public function testGenerateIdHonoursRetryAfter(): void
    {
        $this->http->queue(
            MockHttpClient::fail(429, 'RATE_LIMITED', 'slow down', null, ['Retry-After' => '2']),
            MockHttpClient::ok(['id' => 3]),
        );

        $id = $this->client()->generateId('orders');

        self::assertSame(3, $id);
        self::assertSame([2000], $this->sleeps);
    }

    public function testGenerateIdGivesUpWhenRetryAfterIsTooLong(): void
    {
        // Arrange
        $this->http->queue(MockHttpClient::fail(429, 'RATE_LIMITED', 'slow down', null, ['Retry-After' => '3600']));

        // Act
        try {
            $this->client()->generateId('orders');
            self::fail('expected RateLimitedException');
        } catch (RateLimitedException $e) {
            // Assert
            self::assertSame(3600, $e->retryAfterSeconds());
            self::assertSame(429, $e->status());
        }
        self::assertCount(1, $this->http->requests);
        self::assertSame([], $this->sleeps);
    }

    public function testGenerateIdStopsAfterMaxAttemptsAndThrowsTheLastError(): void
    {
        $this->http->queue(MockHttpClient::fail(500, 'INTERNAL', 'first'), MockHttpClient::fail(503, 'INTERNAL', 'second'));

        try {
            $this->client(retry: $this->recordingRetry(maxAttempts: 2))->generateId('orders');
            self::fail('expected InternalException');
        } catch (InternalException $e) {
            self::assertSame('second', $e->getMessage());
            self::assertSame(503, $e->status());
            self::assertSame(503, $e->getCode());
        }
        self::assertCount(2, $this->http->requests);
    }

    public function testGenerateIdWithoutRetriesThrowsNetworkException(): void
    {
        $this->http->queue(MockHttpClient::networkError('connection refused'));

        $this->expectException(NetworkException::class);
        $this->expectExceptionMessage('connection refused');

        $this->client(retry: RetryPolicy::none())->generateId('orders');
    }

    public function testSequenceExhaustedIsNotRetried(): void
    {
        $this->http->queue(MockHttpClient::fail(409, 'SEQUENCE_EXHAUSTED'));

        try {
            $this->client()->generateId('orders');
            self::fail('expected SequenceExhaustedException');
        } catch (SequenceExhaustedException) {
            self::assertCount(1, $this->http->requests);
        }
    }

    public function testAnEmptyCollectionNameIsRejectedBeforeSending(): void
    {
        $this->expectException(InvalidArgumentException::class);

        try {
            $this->client()->generateId('');
        } finally {
            self::assertSame([], $this->http->requests);
        }
    }

    public function testGenerateIdsRequestsABatch(): void
    {
        // Arrange
        $this->http->queue(MockHttpClient::ok(['first' => 1045, 'last' => 1144, 'count' => 100, 'step' => 1]));

        // Act
        $batch = $this->client()->generateIds('orders', 100);

        // Assert
        self::assertEquals(new IdBatch(1045, 1144, 100, 1), $batch);
        self::assertSame(['count' => '100'], $this->lastQuery());
        self::assertMatchesRegularExpression(self::UUID, $this->lastRequest()->getHeaderLine('Idempotency-Key'));
    }

    public function testGenerateIdsRetriesWithTheSameKey(): void
    {
        $this->http->queue(
            MockHttpClient::networkError(),
            MockHttpClient::ok(['first' => 1, 'last' => 2, 'count' => 2, 'step' => 1]),
        );

        $this->client()->generateIds('orders', 2, 'batch-1');

        self::assertSame(
            ['batch-1', 'batch-1'],
            array_map(static fn ($r): string => $r->getHeaderLine('Idempotency-Key'), $this->http->requests),
        );
    }

    public function testGenerateIdsRejectsACountBelowOne(): void
    {
        $this->expectException(InvalidArgumentException::class);

        $this->client()->generateIds('orders', 0);
    }

    public function testLastIdReturnsLastAndNext(): void
    {
        $this->http->queue(MockHttpClient::ok(['last_id' => 1144, 'next_id' => 1145]));

        $last = $this->client()->lastId('orders');

        self::assertEquals(new LastId(1144, 1145), $last);
        self::assertSame('GET', $this->lastRequest()->getMethod());
        self::assertSame('/v1/collections/orders/last', $this->lastRequest()->getUri()->getPath());
    }

    public function testLastIdPassesNullsThrough(): void
    {
        $this->http->queue(MockHttpClient::ok(['last_id' => null, 'next_id' => 1]));

        self::assertEquals(new LastId(null, 1), $this->client()->lastId('orders'));
    }

    public function testLastIdIsNotRetried(): void
    {
        $this->http->queue(MockHttpClient::fail(500, 'INTERNAL'));

        try {
            $this->client()->lastId('orders');
            self::fail('expected InternalException');
        } catch (InternalException) {
            self::assertCount(1, $this->http->requests);
        }
    }
}
