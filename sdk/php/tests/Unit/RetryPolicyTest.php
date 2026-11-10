<?php

declare(strict_types=1);

namespace IdbSeq\Tests\Unit;

use IdbSeq\Exception\ErrorFactory;
use IdbSeq\Exception\NetworkException;
use IdbSeq\Exception\UnexpectedResponseException;
use IdbSeq\RetryPolicy;
use InvalidArgumentException;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

final class RetryPolicyTest extends TestCase
{
    private static function policy(): RetryPolicy
    {
        return new RetryPolicy(maxAttempts: 5, baseDelayMs: 100, maxDelayMs: 1000, maxRetryAfterMs: 5000);
    }

    public function testDefaultsAreBounded(): void
    {
        $policy = new RetryPolicy();

        self::assertSame(4, $policy->maxAttempts);
        self::assertSame(1, RetryPolicy::none()->maxAttempts);
    }

    /** @return iterable<string, array{int, int, int, int}> */
    public static function invalidSettings(): iterable
    {
        yield 'zero attempts' => [0, 1, 1, 1];
        yield 'negative base delay' => [1, -1, 1, 1];
        yield 'negative max delay' => [1, 1, -1, 1];
        yield 'negative retry-after cap' => [1, 1, 1, -1];
    }

    #[DataProvider('invalidSettings')]
    public function testInvalidSettingsAreRejected(int $attempts, int $base, int $max, int $retryAfter): void
    {
        $this->expectException(InvalidArgumentException::class);

        new RetryPolicy($attempts, $base, $max, $retryAfter);
    }

    public function testNetworkErrorsServerErrorsAndRateLimitsAreRetryable(): void
    {
        $policy = self::policy();

        self::assertTrue($policy->isRetryable(new NetworkException('down')));
        self::assertTrue($policy->isRetryable(ErrorFactory::fromCode('INTERNAL', 'x', 500)));
        self::assertTrue($policy->isRetryable(ErrorFactory::fromCode('RATE_LIMITED', 'x', 429)));
        self::assertTrue($policy->isRetryable(new UnexpectedResponseException('bad gateway', 502, null)));
        self::assertFalse($policy->isRetryable(ErrorFactory::fromCode('SEQUENCE_EXHAUSTED', 'x', 409)));
        self::assertFalse($policy->isRetryable(ErrorFactory::fromCode('VALIDATION_FAILED', 'x', 400)));
    }

    public function testBackoffIsExponentialWithJitterAndCapped(): void
    {
        $policy = self::policy();
        $error = ErrorFactory::fromCode('INTERNAL', 'x', 500);

        self::assertSame(50, $policy->delayMs(1, $error, static fn (): float => 0.0));
        self::assertSame(100, $policy->delayMs(1, $error, static fn (): float => 1.0));
        self::assertSame(400, $policy->delayMs(3, $error, static fn (): float => 1.0));
        self::assertSame(1000, $policy->delayMs(10, $error, static fn (): float => 1.0));
        $random = $policy->delayMs(2, $error);
        self::assertGreaterThanOrEqual(100, $random);
        self::assertLessThanOrEqual(200, $random);
    }

    public function testRetryAfterIsHonouredUpToTheCap(): void
    {
        $policy = self::policy();

        self::assertSame(2000, $policy->delayMs(1, ErrorFactory::fromCode('RATE_LIMITED', 'x', 429, null, null, 2)));
        self::assertNull($policy->delayMs(1, ErrorFactory::fromCode('RATE_LIMITED', 'x', 429, null, null, 60)));
    }

    public function testTheDefaultSleeperWaits(): void
    {
        // Arrange
        $policy = new RetryPolicy(maxAttempts: 2, baseDelayMs: 20, maxDelayMs: 20);
        $calls = 0;
        $started = microtime(true);

        // Act
        $result = $policy->run(static function () use (&$calls): int {
            $calls++;
            if ($calls === 1) {
                throw new NetworkException('down');
            }

            return $calls;
        });

        // Assert
        self::assertSame(2, $result);
        self::assertGreaterThanOrEqual(0.009, microtime(true) - $started);
    }
}
