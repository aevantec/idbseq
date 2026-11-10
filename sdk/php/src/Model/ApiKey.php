<?php

declare(strict_types=1);

namespace IdbSeq\Model;

/** An API key as listed (no secret). `key` is set only right after creation. */
final class ApiKey
{
    /**
     * @param list<string> $scopes
     * @param list<string> $collections allow-list; empty allows every collection
     */
    public function __construct(
        public readonly string $id,
        public readonly string $name,
        public readonly array $scopes,
        public readonly array $collections,
        public readonly string $createdAt,
        public readonly ?string $lastUsedAt,
        /** The full key `idb_<id>_<secret>`, returned only by createKey(). */
        public readonly ?string $key = null,
    ) {
    }
}
