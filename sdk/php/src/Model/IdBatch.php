<?php

declare(strict_types=1);

namespace IdbSeq\Model;

final class IdBatch
{
    public function __construct(
        public readonly int $first,
        public readonly int $last,
        public readonly int $count,
        public readonly int $step,
    ) {
    }
}
