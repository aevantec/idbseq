<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

use Throwable;

/** The server answered with something that is not a valid idbseq envelope. */
final class UnexpectedResponseException extends IdbSeqException
{
    public const CODE = 'UNEXPECTED_RESPONSE';

    public function __construct(string $message, int $status, ?string $requestId, ?Throwable $previous = null)
    {
        parent::__construct($message, self::CODE, $status, null, $requestId, null, $previous);
    }
}
