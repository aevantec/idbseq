<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

use RuntimeException;
use Throwable;

/**
 * Base class of every exception the SDK throws for a failed call.
 *
 * `getCode()` returns the HTTP status (0 when no response was received);
 * `errorCode()` returns the API error code such as `SEQUENCE_EXHAUSTED`.
 */
class IdbSeqException extends RuntimeException
{
    public function __construct(
        string $message,
        private readonly string $errorCode,
        private readonly int $status,
        private readonly mixed $details = null,
        private readonly ?string $requestId = null,
        private readonly ?int $retryAfterSeconds = null,
        ?Throwable $previous = null,
    ) {
        parent::__construct($message, $status, $previous);
    }

    /** API error code, or `NETWORK_ERROR` / `UNEXPECTED_RESPONSE`. */
    public function errorCode(): string
    {
        return $this->errorCode;
    }

    /** HTTP status; 0 when no response was received. */
    public function status(): int
    {
        return $this->status;
    }

    /** `error.details` from the response; for `VALIDATION_FAILED`, `['fields' => [...]]`. */
    public function details(): mixed
    {
        return $this->details;
    }

    /** The `X-Request-Id` response header. */
    public function requestId(): ?string
    {
        return $this->requestId;
    }

    /** Seconds from the `Retry-After` header, when present. */
    public function retryAfterSeconds(): ?int
    {
        return $this->retryAfterSeconds;
    }
}
