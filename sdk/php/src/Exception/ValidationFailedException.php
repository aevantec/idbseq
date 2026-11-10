<?php

declare(strict_types=1);

namespace IdbSeq\Exception;

/** `VALIDATION_FAILED` (400): bad input; details() holds ['fields' => [...]]. */
final class ValidationFailedException extends IdbSeqException
{
}
