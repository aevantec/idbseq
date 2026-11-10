<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

/** `RATE_LIMITED` (429): too many requests; see retryAfterSeconds(). */
final class RateLimitedException extends IdbSeqException
{
}
