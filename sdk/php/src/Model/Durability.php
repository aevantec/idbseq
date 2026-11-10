<?php

declare(strict_types=1);

namespace IdbSeq\Model;

use InvalidArgumentException;

/** How a collection persists issued IDs: `strict` (default) or `block` of `size` IDs. */
final class Durability
{
    public const STRICT = 'strict';
    public const BLOCK = 'block';

    private function __construct(
        public readonly string $mode,
        public readonly ?int $size = null,
    ) {
    }

    public static function strict(): self
    {
        return new self(self::STRICT);
    }

    public static function block(int $size): self
    {
        if ($size < 1) {
            throw new InvalidArgumentException('block size must be >= 1');
        }

        return new self(self::BLOCK, $size);
    }

    /** Keeps an unknown mode from a newer server instead of failing. */
    public static function fromWire(string $mode, ?int $size): self
    {
        return new self($mode, $size);
    }

    /** @return array{mode: string, size?: int} */
    public function toArray(): array
    {
        return $this->size === null ? ['mode' => $this->mode] : ['mode' => $this->mode, 'size' => $this->size];
    }
}
