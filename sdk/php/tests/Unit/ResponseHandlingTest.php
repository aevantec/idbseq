<?php

declare(strict_types=1);

namespace IdbSeq\Tests\Unit;

use IdbSeq\Exception\IdbSeqException;
use IdbSeq\Exception\UnexpectedResponseException;
use IdbSeq\Exception\UnknownCodeException;
use IdbSeq\IdbSeqClient;
use IdbSeq\RetryPolicy;
use IdbSeq\Tests\Support\ClientTestCase;
use IdbSeq\Tests\Support\MockHttpClient;
use InvalidArgumentException;
use Nyholm\Psr7\Factory\Psr17Factory;
use PHPUnit\Framework\Attributes\DataProvider;
use Psr\Http\Message\ResponseInterface;

final class ResponseHandlingTest extends ClientTestCase
{
    private function noRetry(): IdbSeqClient
    {
        return $this->client(retry: RetryPolicy::none());
    }

    private function catchError(callable $call): IdbSeqException
    {
        try {
            $call();
        } catch (IdbSeqException $e) {
            return $e;
        }
        self::fail('expected an IdbSeqException');
    }

    public function testUnknownCodesBecomeUnknownCodeExceptionWithTheHttpStatus(): void
    {
        $this->http->queue(MockHttpClient::fail(418, 'TEAPOT', 'short and stout'));

        $error = $this->catchError(fn () => $this->noRetry()->getCollection('a'));

        self::assertInstanceOf(UnknownCodeException::class, $error);
        self::assertSame('TEAPOT', $error->errorCode());
        self::assertSame(418, $error->status());
        self::assertNull($error->details());
    }

    /** @return iterable<string, array{ResponseInterface}> */
    public static function malformedResponses(): iterable
    {
        yield 'non-JSON body' => [MockHttpClient::raw(502, '<html>Bad Gateway</html>')];
        yield 'JSON without envelope' => [MockHttpClient::raw(200, '{"id":1}')];
        yield 'success envelope with error status' => [MockHttpClient::ok(['id' => 1], null, 500)];
        yield 'error envelope without code' => [MockHttpClient::raw(400, '{"success":false,"data":null,"error":null}')];
        yield 'data of the wrong type' => [MockHttpClient::ok(['id' => 'one'])];
        yield 'data that is a list' => [MockHttpClient::ok([1, 2])];
    }

    #[DataProvider('malformedResponses')]
    public function testMalformedResponsesBecomeUnexpectedResponseException(ResponseInterface $response): void
    {
        $this->http->queue($response);

        $error = $this->catchError(fn () => $this->noRetry()->generateId('a'));

        self::assertInstanceOf(UnexpectedResponseException::class, $error);
        self::assertSame('UNEXPECTED_RESPONSE', $error->errorCode());
        self::assertSame($response->getStatusCode(), $error->status());
    }

    public function testAnErrorMessageDefaultsToTheCode(): void
    {
        $this->http->queue(MockHttpClient::raw(500, '{"success":false,"data":null,"error":{"code":"INTERNAL"}}'));

        $error = $this->catchError(fn () => $this->noRetry()->lastId('a'));

        self::assertSame('INTERNAL', $error->getMessage());
    }

    public function testRetryAfterAsAnHttpDateIsParsed(): void
    {
        $inTenSeconds = gmdate('D, d M Y H:i:s \G\M\T', time() + 10);
        $this->http->queue(MockHttpClient::fail(429, 'RATE_LIMITED', 'slow', null, ['Retry-After' => $inTenSeconds]));

        $error = $this->catchError(fn () => $this->noRetry()->lastId('a'));

        self::assertGreaterThanOrEqual(8, $error->retryAfterSeconds());
        self::assertLessThanOrEqual(10, $error->retryAfterSeconds());
    }

    public function testAMalformedRetryAfterIsIgnored(): void
    {
        $this->http->queue(MockHttpClient::fail(429, 'RATE_LIMITED', 'slow', null, ['Retry-After' => 'soon']));

        $error = $this->catchError(fn () => $this->noRetry()->lastId('a'));

        self::assertNull($error->retryAfterSeconds());
    }

    public function testABaseUrlPathPrefixIsKept(): void
    {
        $factory = new Psr17Factory();
        $client = new IdbSeqClient('https://ids.example.com/idbseq/', null, $this->http, $factory, $factory);
        $this->http->queue(MockHttpClient::ok(['last_id' => null, 'next_id' => 1]));

        $client->lastId('a');

        self::assertSame('https://ids.example.com/idbseq/v1/collections/a/last', (string) $this->lastRequest()->getUri());
        self::assertFalse($this->lastRequest()->hasHeader('Authorization'));
        self::assertStringStartsWith('idbseq-php/', $this->lastRequest()->getHeaderLine('User-Agent'));
    }

    /** @return iterable<string, array{string}> */
    public static function invalidUrls(): iterable
    {
        yield 'not a URL' => ['not a url'];
        yield 'wrong scheme' => ['ftp://example.com'];
        yield 'no host' => ['http://'];
    }

    #[DataProvider('invalidUrls')]
    public function testInvalidBaseUrlsAreRejected(string $url): void
    {
        $this->expectException(InvalidArgumentException::class);

        new IdbSeqClient($url, null, $this->http, new Psr17Factory(), new Psr17Factory());
    }

    public function testDiscoveryFindsAnInstalledHttpClient(): void
    {
        $this->expectNotToPerformAssertions();

        new IdbSeqClient();
    }
}
