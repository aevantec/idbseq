<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

use Throwable;

/** The request never got a response (connection failure or timeout in the PSR-18 client). */
final class NetworkException extends IdbSeqException
{
    public const CODE = 'NETWORK_ERROR';

    public function __construct(string $message, ?Throwable $previous = null)
    {
        parent::__construct($message, self::CODE, 0, null, null, null, $previous);
    }
}
