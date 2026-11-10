<?php

declare(strict_types=1);

namespace IdbSeq\Tests\Unit;

use IdbSeq\Exception\NotFoundException;
use IdbSeq\Exception\UnauthorizedException;
use IdbSeq\Exception\UnexpectedResponseException;
use IdbSeq\Model\ApiKey;
use IdbSeq\Model\Session;
use IdbSeq\Tests\Support\ClientTestCase;
use IdbSeq\Tests\Support\MockHttpClient;
use InvalidArgumentException;

final class AdminTest extends ClientTestCase
{
    private const WIRE_KEY = [
        'id' => 'k7f3a9c2',
        'name' => 'billing-service',
        'scopes' => ['read', 'generate'],
        'collections' => ['orders', 'eu.*'],
        'created_at' => '2026-11-10T08:15:30Z',
        'last_used_at' => null,
    ];

    /** @param list<string>|null $collections */
    /** @param list<string>|null $collections */
    private static function expectedKey(?string $key = null, ?array $collections = null): ApiKey
    {
        return new ApiKey(
            'k7f3a9c2',
            'billing-service',
            ['read', 'generate'],
            $collections ?? ['orders', 'eu.*'],
            '2026-11-10T08:15:30Z',
            null,
            $key,
        );
    }

    public function testLoginSendsCredentialsWithoutAuthorizationAndStoresTheToken(): void
    {
        // Arrange
        $this->http->queue(
            MockHttpClient::ok(['token' => 'sess-1', 'expires_at' => '2026-11-10T08:30:30Z']),
            MockHttpClient::ok(['id' => 1]),
        );
        $client = $this->client(apiKey: null);

        // Act
        $session = $client->login('admin', 'correct horse');
        $client->generateId('orders');

        // Assert
        self::assertEquals(new Session('sess-1', '2026-11-10T08:30:30Z'), $session);
        [$login, $generate] = $this->http->requests;
        self::assertSame('/v1/auth/login', $login->getUri()->getPath());
        self::assertFalse($login->hasHeader('Authorization'));
        self::assertSame('{"username":"admin","password":"correct horse"}', (string) $login->getBody());
        self::assertSame('Bearer sess-1', $generate->getHeaderLine('Authorization'));
    }

    public function testFailedLoginThrowsUnauthorizedAndKeepsNoToken(): void
    {
        $this->http->queue(MockHttpClient::fail(401, 'UNAUTHORIZED'), MockHttpClient::ok(['id' => 1]));
        $client = $this->client(apiKey: null);

        try {
            $client->login('admin', 'wrong');
            self::fail('expected UnauthorizedException');
        } catch (UnauthorizedException) {
            $client->generateId('orders');
        }

        self::assertFalse($this->lastRequest()->hasHeader('Authorization'));
    }

    public function testLogoutRevokesTheSessionAndFallsBackToTheApiKey(): void
    {
        // Arrange
        $this->http->queue(
            MockHttpClient::ok(['token' => 'sess-2', 'expires_at' => 'x']),
            MockHttpClient::ok(null),
            MockHttpClient::ok(['id' => 1]),
        );
        $client = $this->client(apiKey: 'idb_k_s');
        $client->login('admin', 'pw');

        // Act
        $client->logout();
        $client->generateId('orders');

        // Assert
        self::assertSame('/v1/auth/logout', $this->http->requests[1]->getUri()->getPath());
        self::assertSame('Bearer sess-2', $this->http->requests[1]->getHeaderLine('Authorization'));
        self::assertSame('Bearer idb_k_s', $this->lastRequest()->getHeaderLine('Authorization'));
    }

    public function testLogoutWithoutASessionSendsNothing(): void
    {
        $this->client()->logout();

        self::assertSame([], $this->http->requests);
    }

    public function testCreateKeyReturnsTheSecretOnce(): void
    {
        $this->http->queue(MockHttpClient::ok([...self::WIRE_KEY, 'key' => 'idb_k7f3a9c2_secret'], null, 201));

        $key = $this->client()->createKey('billing-service', ['read', 'generate'], ['orders', 'eu.*']);

        self::assertEquals(self::expectedKey('idb_k7f3a9c2_secret'), $key);
        self::assertSame('/v1/keys', $this->lastRequest()->getUri()->getPath());
        self::assertSame(
            '{"name":"billing-service","scopes":["read","generate"],"collections":["orders","eu.*"]}',
            $this->lastBody(),
        );
    }

    public function testCreateKeyOmitsAMissingAllowList(): void
    {
        $this->http->queue(MockHttpClient::ok([...self::WIRE_KEY, 'key' => 'k']));

        $this->client()->createKey('svc', ['read']);

        self::assertSame('{"name":"svc","scopes":["read"]}', $this->lastBody());
    }

    public function testListKeysTreatsAMissingAllowListAsEmpty(): void
    {
        $withoutCollections = self::WIRE_KEY;
        unset($withoutCollections['collections']);
        $this->http->queue(MockHttpClient::ok([self::WIRE_KEY, $withoutCollections]));

        $keys = $this->client()->listKeys();

        self::assertEquals([self::expectedKey(), self::expectedKey(null, [])], $keys);
    }

    public function testListKeysRejectsMalformedScopes(): void
    {
        $this->http->queue(MockHttpClient::ok([[...self::WIRE_KEY, 'scopes' => [1]]]));

        $this->expectException(UnexpectedResponseException::class);
        $this->expectExceptionMessage('data[0].scopes[0]');

        $this->client()->listKeys();
    }

    public function testRevokeKeySendsDelete(): void
    {
        $this->http->queue(MockHttpClient::ok(null));

        $this->client()->revokeKey('k7f3a9c2');

        self::assertSame('DELETE', $this->lastRequest()->getMethod());
        self::assertSame('/v1/keys/k7f3a9c2', $this->lastRequest()->getUri()->getPath());
    }

    public function testRevokeKeyThrowsNotFoundForAnUnknownId(): void
    {
        $this->http->queue(MockHttpClient::fail(404, 'NOT_FOUND'));

        $this->expectException(NotFoundException::class);

        $this->client()->revokeKey('nope');
    }

    public function testRevokeKeyRejectsAnEmptyId(): void
    {
        $this->expectException(InvalidArgumentException::class);

        $this->client()->revokeKey('');
    }
}
