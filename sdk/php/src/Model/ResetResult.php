<?php

declare(strict_types=1);

namespace IdbSeq\Model;

final class ResetResult
{
    public function __construct(
        public readonly ?int $previousLastId,
        public readonly ?int $lastId,
        public readonly ?int $nextId,
    ) {
    }
}
