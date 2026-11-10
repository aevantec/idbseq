<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

/** `FORBIDDEN` (403): the credentials lack the scope, or the collection is outside the key's allow-list. */
final class ForbiddenException extends IdbSeqException
{
}
