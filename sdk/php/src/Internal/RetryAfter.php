<?php

declare(strict_types=1);

namespace IdbSeq\Internal;

/** @internal Parses a `Retry-After` header given as delta-seconds or an HTTP date. */
final class RetryAfter
{
    private function __construct()
    {
    }

    public static function parse(string $header, int $now): ?int
    {
        $value = trim($header);
        if ($value === '') {
            return null;
        }
        if (ctype_digit($value)) {
            return (int) $value;
        }
        $at = strtotime($value);

        return $at === false ? null : max(0, $at - $now);
    }
}
