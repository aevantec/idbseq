<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

/** `UNAUTHORIZED` (401): missing, unknown, expired or revoked credentials, or a failed login. */
final class UnauthorizedException extends IdbSeqException
{
}
