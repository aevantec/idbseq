<?php

declare(strict_types=1);

namespace IdbSeq\Model;

final class Session
{
    public function __construct(
        public readonly string $token,
        public readonly string $expiresAt,
    ) {
    }
}
