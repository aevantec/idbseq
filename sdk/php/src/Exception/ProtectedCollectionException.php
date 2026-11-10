<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

/** `PROTECTED_COLLECTION` (403): the operation needs the admin because the collection is protected. */
final class ProtectedCollectionException extends IdbSeqException
{
}
