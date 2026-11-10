<?php

declare(strict_types=1);

namespace IdbSeq\Tests\Unit;

use IdbSeq\Exception\CollectionExistsException;
use IdbSeq\Exception\CollectionNotFoundException;
use IdbSeq\Exception\ErrorFactory;
use IdbSeq\Exception\ForbiddenException;
use IdbSeq\Exception\IdbSeqException;
use IdbSeq\Exception\InternalException;
use IdbSeq\Exception\NetworkException;
use IdbSeq\Exception\NotFoundException;
use IdbSeq\Exception\PayloadTooLargeException;
use IdbSeq\Exception\ProtectedCollectionException;
use IdbSeq\Exception\RateLimitedException;
use IdbSeq\Exception\SequenceExhaustedException;
use IdbSeq\Exception\UnauthorizedException;
use IdbSeq\Exception\UnknownCodeException;
use IdbSeq\Exception\ValidationFailedException;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

final class ErrorFactoryTest extends TestCase
{
    /** @return iterable<string, array{string, int, class-string<IdbSeqException>}> */
    public static function knownCodes(): iterable
    {
        yield 'VALIDATION_FAILED' => ['VALIDATION_FAILED', 400, ValidationFailedException::class];
        yield 'UNAUTHORIZED' => ['UNAUTHORIZED', 401, UnauthorizedException::class];
        yield 'FORBIDDEN' => ['FORBIDDEN', 403, ForbiddenException::class];
        yield 'PROTECTED_COLLECTION' => ['PROTECTED_COLLECTION', 403, ProtectedCollectionException::class];
        yield 'NOT_FOUND' => ['NOT_FOUND', 404, NotFoundException::class];
        yield 'COLLECTION_NOT_FOUND' => ['COLLECTION_NOT_FOUND', 404, CollectionNotFoundException::class];
        yield 'COLLECTION_EXISTS' => ['COLLECTION_EXISTS', 409, CollectionExistsException::class];
        yield 'SEQUENCE_EXHAUSTED' => ['SEQUENCE_EXHAUSTED', 409, SequenceExhaustedException::class];
        yield 'PAYLOAD_TOO_LARGE' => ['PAYLOAD_TOO_LARGE', 413, PayloadTooLargeException::class];
        yield 'RATE_LIMITED' => ['RATE_LIMITED', 429, RateLimitedException::class];
        yield 'INTERNAL' => ['INTERNAL', 500, InternalException::class];
    }

    /** @param class-string<IdbSeqException> $class */
    #[DataProvider('knownCodes')]
    public function testEachCodeMapsToItsOwnException(string $code, int $status, string $class): void
    {
        $error = ErrorFactory::fromCode($code, 'boom', $status, ['k' => 'v'], 'req-1', 3);

        self::assertInstanceOf($class, $error);
        self::assertSame($code, $error->errorCode());
        self::assertSame($status, $error->status());
        self::assertSame(['k' => 'v'], $error->details());
        self::assertSame('req-1', $error->requestId());
        self::assertSame(3, $error->retryAfterSeconds());
        self::assertSame('boom', $error->getMessage());
    }

    public function testUnknownCodesFallBackToUnknownCodeException(): void
    {
        $error = ErrorFactory::fromCode('BRAND_NEW', 'x', 418);

        self::assertInstanceOf(UnknownCodeException::class, $error);
        self::assertSame(418, $error->status());
    }

    public function testNetworkExceptionHasStatusZero(): void
    {
        $error = new NetworkException('down');

        self::assertSame('NETWORK_ERROR', $error->errorCode());
        self::assertSame(0, $error->status());
        self::assertNull($error->requestId());
    }
}
