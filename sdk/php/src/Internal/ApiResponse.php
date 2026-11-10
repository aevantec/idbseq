<?php

declare(strict_types=1);

namespace IdbSeq\Internal;

/** @internal An unwrapped success envelope. */
final class ApiResponse
{
    public function __construct(
        public readonly mixed $data,
        public readonly mixed $meta,
        public readonly int $status,
        public readonly ?string $requestId,
    ) {
    }
}
