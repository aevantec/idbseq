<?php

declare(strict_types=1);

namespace IdbSeq\Model;

final class CollectionPage
{
    /**
     * @param list<Collection> $items
     */
    public function __construct(
        public readonly array $items,
        public readonly int $total,
        public readonly int $limit,
        /** Pass back as `cursor` for the next page; null on the last page. */
        public readonly ?string $nextCursor,
    ) {
    }
}
