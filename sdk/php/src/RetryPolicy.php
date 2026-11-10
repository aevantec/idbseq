<?php

declare(strict_types=1);

namespace IdbSeq;

use IdbSeq\Exception\IdbSeqException;
use IdbSeq\Exception\NetworkException;
use Closure;
use InvalidArgumentException;

/**
 * Retry settings for generateId() / generateIds(): network failures, 5xx and 429
 * are retried with the same idempotency key, with exponential backoff and jitter.
 */
final class RetryPolicy
{
    private const HTTP_TOO_MANY_REQUESTS = 429;
    private const HTTP_SERVER_ERROR = 500;
    private const MS_PER_SECOND = 1000;
    private const US_PER_MS = 1000;

    /** @var Closure(int): void */
    private readonly Closure $sleeper;

    /**
     * @param int $maxAttempts total attempts including the first one
     * @param int $baseDelayMs first backoff delay; doubles per attempt
     * @param int $maxDelayMs upper bound of the exponential backoff
     * @param int $maxRetryAfterMs a longer Retry-After is not waited for; the exception is thrown
     * @param (Closure(int): void)|null $sleeper waits the given milliseconds (for tests)
     */
    public function __construct(
        public readonly int $maxAttempts = 4,
        public readonly int $baseDelayMs = 100,
        public readonly int $maxDelayMs = 2000,
        public readonly int $maxRetryAfterMs = 30000,
        ?Closure $sleeper = null,
    ) {
        if ($maxAttempts < 1) {
            throw new InvalidArgumentException('maxAttempts must be >= 1');
        }
        if ($baseDelayMs < 0 || $maxDelayMs < 0 || $maxRetryAfterMs < 0) {
            throw new InvalidArgumentException('retry delays must be >= 0');
        }
        $this->sleeper = $sleeper ?? static function (int $ms): void {
            usleep($ms * self::US_PER_MS);
        };
    }

    public static function none(): self
    {
        return new self(maxAttempts: 1);
    }

    public function isRetryable(IdbSeqException $error): bool
    {
        return $error instanceof NetworkException
            || $error->status() >= self::HTTP_SERVER_ERROR
            || $error->status() === self::HTTP_TOO_MANY_REQUESTS;
    }

    /**
     * Delay in milliseconds after `$attempt` failed attempts, or null when the
     * server asks to wait longer than allowed.
     *
     * @param (Closure(): float)|null $random returns a float in [0, 1]
     */
    public function delayMs(int $attempt, IdbSeqException $error, ?Closure $random = null): ?int
    {
        $retryAfter = $error->retryAfterSeconds();
        if ($retryAfter !== null) {
            $requested = $retryAfter * self::MS_PER_SECOND;

            return $requested > $this->maxRetryAfterMs ? null : $requested;
        }
        $ceiling = min($this->maxDelayMs, $this->baseDelayMs * 2 ** ($attempt - 1));
        $fraction = $random !== null ? $random() : mt_rand() / mt_getrandmax();

        return (int) round($ceiling / 2 + $fraction * $ceiling / 2);
    }

    /**
     * Runs `$operation` until it succeeds, fails with a non-retryable error, or
     * attempts run out.
     *
     * @template T
     * @param Closure(): T $operation
     * @return T
     */
    public function run(Closure $operation): mixed
    {
        for ($attempt = 1; ; $attempt++) {
            try {
                return $operation();
            } catch (IdbSeqException $error) {
                $delay = $attempt < $this->maxAttempts && $this->isRetryable($error)
                    ? $this->delayMs($attempt, $error)
                    : null;
                if ($delay === null) {
                    throw $error;
                }
                ($this->sleeper)($delay);
            }
        }
    }
}
