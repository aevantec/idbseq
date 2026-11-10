<?php

declare(strict_types=1);

namespace IdbSeq\Internal;

use IdbSeq\Model\ApiKey;
use IdbSeq\Model\Collection;
use IdbSeq\Model\CollectionPage;
use IdbSeq\Model\Durability;
use IdbSeq\Model\IdBatch;
use IdbSeq\Model\LastId;
use IdbSeq\Model\ResetResult;
use IdbSeq\Model\Session;

/** @internal Turns snake_case response data into model objects. */
final class Decoder
{
    private function __construct()
    {
    }

    public static function collection(mixed $data, string $path = 'data'): Collection
    {
        $f = Fields::of($data, $path);
        $durability = $f->object('durability');

        return new Collection(
            name: $f->string('name'),
            start: $f->int('start'),
            step: $f->int('step'),
            max: $f->int('max'),
            protected: $f->bool('protected'),
            durability: Durability::fromWire($durability->string('mode'), $durability->nullableInt('size')),
            createdAt: $f->string('created_at'),
            lastId: $f->nullableInt('last_id'),
            nextId: $f->nullableInt('next_id'),
        );
    }

    public static function collectionPage(mixed $data, mixed $meta): CollectionPage
    {
        $items = [];
        foreach (Fields::list($data, 'data') as $index => $item) {
            $items[] = self::collection($item, "data[{$index}]");
        }
        if ($meta === null) {
            return new CollectionPage($items, count($items), count($items), null);
        }
        $m = Fields::of($meta, 'meta');

        return new CollectionPage($items, $m->int('total'), $m->int('limit'), $m->nullableString('next_cursor'));
    }

    public static function generatedId(mixed $data): int
    {
        return Fields::of($data, 'data')->int('id');
    }

    public static function batch(mixed $data): IdBatch
    {
        $f = Fields::of($data, 'data');

        return new IdBatch($f->int('first'), $f->int('last'), $f->int('count'), $f->int('step'));
    }

    public static function lastId(mixed $data): LastId
    {
        $f = Fields::of($data, 'data');

        return new LastId($f->nullableInt('last_id'), $f->nullableInt('next_id'));
    }

    public static function resetResult(mixed $data): ResetResult
    {
        $f = Fields::of($data, 'data');

        return new ResetResult($f->nullableInt('previous_last_id'), $f->nullableInt('last_id'), $f->nullableInt('next_id'));
    }

    public static function apiKey(mixed $data, string $path = 'data'): ApiKey
    {
        $f = Fields::of($data, $path);

        return new ApiKey(
            id: $f->string('id'),
            name: $f->string('name'),
            scopes: $f->strings('scopes'),
            collections: $f->strings('collections'),
            createdAt: $f->string('created_at'),
            lastUsedAt: $f->nullableString('last_used_at'),
            key: $f->nullableString('key'),
        );
    }

    /** @return list<ApiKey> */
    public static function apiKeys(mixed $data): array
    {
        $keys = [];
        foreach (Fields::list($data, 'data') as $index => $item) {
            $keys[] = self::apiKey($item, "data[{$index}]");
        }

        return $keys;
    }

    public static function session(mixed $data): Session
    {
        $f = Fields::of($data, 'data');

        return new Session($f->string('token'), $f->string('expires_at'));
    }
}
