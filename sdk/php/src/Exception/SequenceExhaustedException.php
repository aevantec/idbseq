<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

/** `SEQUENCE_EXHAUSTED` (409): the request would pass the collection's max; nothing was issued. */
final class SequenceExhaustedException extends IdbSeqException
{
}
