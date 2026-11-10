<?php

declare(strict_types=1);

namespace IdbSeq\Internal;

use UnexpectedValueException;

/**
 * @internal Typed, path-aware access to a decoded JSON object. Every accessor
 * throws UnexpectedValueException naming the field when the type is wrong.
 */
final class Fields
{
    /** @param array<string, mixed> $values */
    private function __construct(private readonly array $values, private readonly string $path)
    {
    }

    public static function of(mixed $value, string $path): self
    {
        if (!is_array($value) || ($value !== [] && array_is_list($value))) {
            throw new UnexpectedValueException("expected an object at {$path}");
        }
        /** @var array<string, mixed> $value */
        return new self($value, $path);
    }

    /** @return list<mixed> */
    public static function list(mixed $value, string $path): array
    {
        if (!is_array($value) || !array_is_list($value)) {
            throw new UnexpectedValueException("expected an array at {$path}");
        }

        return $value;
    }

    public function string(string $key): string
    {
        $value = $this->values[$key] ?? null;
        if (!is_string($value)) {
            throw new UnexpectedValueException("expected a string at {$this->at($key)}");
        }

        return $value;
    }

    public function nullableString(string $key): ?string
    {
        return ($this->values[$key] ?? null) === null ? null : $this->string($key);
    }

    public function int(string $key): int
    {
        $value = $this->values[$key] ?? null;
        if (!is_int($value)) {
            throw new UnexpectedValueException("expected an integer at {$this->at($key)}");
        }

        return $value;
    }

    public function nullableInt(string $key): ?int
    {
        return ($this->values[$key] ?? null) === null ? null : $this->int($key);
    }

    public function bool(string $key): bool
    {
        $value = $this->values[$key] ?? null;
        if (!is_bool($value)) {
            throw new UnexpectedValueException("expected a boolean at {$this->at($key)}");
        }

        return $value;
    }

    public function object(string $key): self
    {
        return self::of($this->values[$key] ?? null, $this->at($key));
    }

    /** @return list<string> a missing or null value is an empty list */
    public function strings(string $key): array
    {
        $value = $this->values[$key] ?? null;
        if ($value === null) {
            return [];
        }
        $items = self::list($value, $this->at($key));
        foreach ($items as $index => $item) {
            if (!is_string($item)) {
                throw new UnexpectedValueException("expected a string at {$this->at($key)}[{$index}]");
            }
        }

        /** @var list<string> $items */
        return $items;
    }

    private function at(string $key): string
    {
        return "{$this->path}.{$key}";
    }
}
