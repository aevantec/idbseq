<?php

declare(strict_types=1);

namespace IdbSeq\Tests\Unit;

use IdbSeq\Exception\CollectionExistsException;
use IdbSeq\Exception\CollectionNotFoundException;
use IdbSeq\Exception\ProtectedCollectionException;
use IdbSeq\Exception\ValidationFailedException;
use IdbSeq\Model\Collection;
use IdbSeq\Model\Durability;
use IdbSeq\Model\ResetResult;
use IdbSeq\Tests\Support\ClientTestCase;
use IdbSeq\Tests\Support\MockHttpClient;

final class CollectionsTest extends ClientTestCase
{
    private const WIRE = [
        'name' => 'orders',
        'start' => 1,
        'step' => 1,
        'max' => 1000,
        'protected' => false,
        'durability' => ['mode' => 'strict'],
        'created_at' => '2026-11-10T08:15:30Z',
        'last_id' => 1043,
        'next_id' => 1044,
    ];

    private static function expected(): Collection
    {
        return new Collection('orders', 1, 1, 1000, false, Durability::strict(), '2026-11-10T08:15:30Z', 1043, 1044);
    }

    public function testListCollectionsReturnsItemsTotalAndCursor(): void
    {
        // Arrange
        $this->http->queue(MockHttpClient::ok([self::WIRE], ['total' => 120, 'limit' => 1, 'next_cursor' => 'orders']));

        // Act
        $page = $this->client()->listCollections(limit: 1, cursor: 'a', prefix: 'or');

        // Assert
        self::assertEquals([self::expected()], $page->items);
        self::assertSame(120, $page->total);
        self::assertSame(1, $page->limit);
        self::assertSame('orders', $page->nextCursor);
        self::assertSame('/v1/collections', $this->lastRequest()->getUri()->getPath());
        self::assertSame(['limit' => '1', 'cursor' => 'a', 'prefix' => 'or'], $this->lastQuery());
    }

    public function testListCollectionsSendsNoQueryByDefault(): void
    {
        $this->http->queue(MockHttpClient::ok([], ['total' => 0, 'limit' => 50, 'next_cursor' => null]));

        $page = $this->client()->listCollections();

        self::assertSame([], $page->items);
        self::assertNull($page->nextCursor);
        self::assertSame('', $this->lastRequest()->getUri()->getQuery());
    }

    public function testListCollectionsWithoutMetaIsASingleCompletePage(): void
    {
        $block = [...self::WIRE, 'max' => PHP_INT_MAX, 'durability' => ['mode' => 'block', 'size' => 1000]];
        $this->http->queue(MockHttpClient::ok([$block]));

        $page = $this->client()->listCollections();

        self::assertSame(1, $page->total);
        self::assertNull($page->nextCursor);
        self::assertSame(PHP_INT_MAX, $page->items[0]->max);
        self::assertEquals(Durability::block(1000), $page->items[0]->durability);
    }

    public function testBlockDurabilityNeedsAPositiveSize(): void
    {
        $this->expectException(\InvalidArgumentException::class);

        Durability::block(0);
    }

    public function testGetCollectionMapsToTheModel(): void
    {
        $this->http->queue(MockHttpClient::ok(self::WIRE));

        self::assertEquals(self::expected(), $this->client()->getCollection('orders'));
        self::assertSame('/v1/collections/orders', $this->lastRequest()->getUri()->getPath());
    }

    public function testGetCollectionThrowsCollectionNotFound(): void
    {
        $this->http->queue(MockHttpClient::fail(404, 'COLLECTION_NOT_FOUND'));

        $this->expectException(CollectionNotFoundException::class);

        $this->client()->getCollection('nope');
    }

    public function testCreateCollectionSendsTheSpecInSnakeCase(): void
    {
        // Arrange
        $this->http->queue(MockHttpClient::ok(self::WIRE, null, 201));

        // Act
        $created = $this->client()->createCollection(
            name: 'orders',
            start: 1,
            step: 1,
            max: PHP_INT_MAX,
            protected: true,
            durability: Durability::block(100),
        );

        // Assert
        self::assertEquals(self::expected(), $created);
        self::assertSame('POST', $this->lastRequest()->getMethod());
        self::assertSame('application/json', $this->lastRequest()->getHeaderLine('Content-Type'));
        self::assertSame(
            '{"name":"orders","start":1,"step":1,"max":9223372036854775807,"protected":true,'
            . '"durability":{"mode":"block","size":100}}',
            $this->lastBody(),
        );
    }

    public function testCreateCollectionOmitsFieldsThatAreNotGiven(): void
    {
        $this->http->queue(MockHttpClient::ok(self::WIRE));

        $this->client()->createCollection('orders');

        self::assertSame('{"name":"orders"}', $this->lastBody());
    }

    public function testCreateCollectionThrowsCollectionExists(): void
    {
        $this->http->queue(MockHttpClient::fail(409, 'COLLECTION_EXISTS'));

        $this->expectException(CollectionExistsException::class);

        $this->client()->createCollection('orders');
    }

    public function testValidationDetailsAndRequestIdAreExposed(): void
    {
        $this->http->queue(MockHttpClient::fail(400, 'VALIDATION_FAILED', 'invalid', ['fields' => ['name' => 'bad']]));

        try {
            $this->client()->createCollection('Bad!');
            self::fail('expected ValidationFailedException');
        } catch (ValidationFailedException $e) {
            self::assertSame(['fields' => ['name' => 'bad']], $e->details());
            self::assertSame('req-test-1', $e->requestId());
            self::assertSame('VALIDATION_FAILED', $e->errorCode());
        }
    }

    public function testUpdateCollectionPatchesOnlyGivenFields(): void
    {
        $this->http->queue(MockHttpClient::ok(self::WIRE));

        $this->client()->updateCollection('orders', max: 5000, protected: false);

        self::assertSame('PATCH', $this->lastRequest()->getMethod());
        self::assertSame('{"max":5000,"protected":false}', $this->lastBody());
    }

    public function testUpdateCollectionWithNothingSendsAnEmptyObject(): void
    {
        $this->http->queue(MockHttpClient::ok(self::WIRE));

        $this->client()->updateCollection('orders', durability: null);

        self::assertSame('{}', $this->lastBody());
    }

    public function testResetCollectionSendsConfirm(): void
    {
        // Arrange
        $this->http->queue(MockHttpClient::ok(['previous_last_id' => 1144, 'last_id' => null, 'next_id' => 1]));

        // Act
        $result = $this->client()->resetCollection('orders');

        // Assert
        self::assertEquals(new ResetResult(1144, null, 1), $result);
        self::assertSame('/v1/collections/orders/reset', $this->lastRequest()->getUri()->getPath());
        self::assertSame('{"confirm":"orders"}', $this->lastBody());
    }

    public function testResetCollectionSendsValueAndAllowRewind(): void
    {
        $this->http->queue(MockHttpClient::ok(['previous_last_id' => 50, 'last_id' => 9, 'next_id' => 10]));

        $this->client()->resetCollection('orders', value: 10, allowRewind: true);

        self::assertSame('{"confirm":"orders","value":10,"allow_rewind":true}', $this->lastBody());
    }

    public function testResetCollectionThrowsProtectedCollection(): void
    {
        $this->http->queue(MockHttpClient::fail(403, 'PROTECTED_COLLECTION'));

        $this->expectException(ProtectedCollectionException::class);

        $this->client()->resetCollection('tickets');
    }

    public function testDeleteCollectionSendsConfirmAndReturnsTheCollection(): void
    {
        $this->http->queue(MockHttpClient::ok(self::WIRE));

        $deleted = $this->client()->deleteCollection('orders');

        self::assertEquals(self::expected(), $deleted);
        self::assertSame('DELETE', $this->lastRequest()->getMethod());
        self::assertSame(['confirm' => 'orders'], $this->lastQuery());
    }
}
