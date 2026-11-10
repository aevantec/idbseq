<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

/** Maps API error codes to exception classes. */
final class ErrorFactory
{
    /** @var array<string, class-string<IdbSeqException>> */
    private const CLASSES = [
        'VALIDATION_FAILED' => ValidationFailedException::class,
        'UNAUTHORIZED' => UnauthorizedException::class,
        'FORBIDDEN' => ForbiddenException::class,
        'PROTECTED_COLLECTION' => ProtectedCollectionException::class,
        'NOT_FOUND' => NotFoundException::class,
        'COLLECTION_NOT_FOUND' => CollectionNotFoundException::class,
        'COLLECTION_EXISTS' => CollectionExistsException::class,
        'SEQUENCE_EXHAUSTED' => SequenceExhaustedException::class,
        'PAYLOAD_TOO_LARGE' => PayloadTooLargeException::class,
        'RATE_LIMITED' => RateLimitedException::class,
        'INTERNAL' => InternalException::class,
    ];

    private function __construct()
    {
    }

    public static function fromCode(
        string $code,
        string $message,
        int $status,
        mixed $details = null,
        ?string $requestId = null,
        ?int $retryAfterSeconds = null,
    ): IdbSeqException {
        $class = self::CLASSES[$code] ?? UnknownCodeException::class;

        return new $class($message, $code, $status, $details, $requestId, $retryAfterSeconds);
    }
}
