<?php

declare(strict_types=1);

namespace IdbSeq\Model;

final class Collection
{
    public function __construct(
        public readonly string $name,
        public readonly int $start,
        public readonly int $step,
        public readonly int $max,
        public readonly bool $protected,
        public readonly Durability $durability,
        /** RFC 3339 timestamp in UTC. */
        public readonly string $createdAt,
        /** Most recently issued ID; null before the first. */
        public readonly ?int $lastId,
        /** What the next call would return (not reserved); null when exhausted. */
        public readonly ?int $nextId,
    ) {
    }
}
