// Pure form helpers: parse what the user typed into values the API accepts.

import type { Id } from './types';

export const I64_MIN = -(2n ** 63n);
export const I64_MAX = 2n ** 63n - 1n;
/** `step` is a u64 on the server. */
export const STEP_MAX = 2n ** 64n - 1n;
/** A block size is a u32 on the server. */
export const BLOCK_SIZE_MAX = 2n ** 32n - 1n;
/** `?count=` is a u32 on the server (and capped by `limits.max_batch`). */
export const COUNT_MAX = 2n ** 32n - 1n;

const INTEGER = /^-?\d+$/;
const LIST_SEPARATOR = /[\s,]+/;

export type Parsed<T> = { value: T; error?: undefined } | { error: string; value?: undefined };

export interface IntegerBounds {
  label?: string;
  min?: bigint | number;
  max?: bigint | number;
}

/**
 * Parse a whole number typed into a field: a Number when exact, else a
 * BigInt. Bounds default to the signed 64-bit range.
 */
export function parseInteger(
  text: unknown,
  { label = 'Value', min = I64_MIN, max = I64_MAX }: IntegerBounds = {},
): Parsed<Id> {
  const trimmed = String(text ?? '').trim();
  if (!INTEGER.test(trimmed)) return { error: `${label} must be a whole number.` };
  const big = BigInt(trimmed);
  if (big < BigInt(min)) return { error: `${label} must be at least ${min}.` };
  if (big > BigInt(max)) return { error: `${label} must be at most ${max}.` };
  const number = Number(big);
  return { value: Number.isSafeInteger(number) ? number : big };
}

/** Split "a, b\nc" into ["a", "b", "c"], without blanks or repeats. */
export function parseList(text: unknown): string[] {
  const items = String(text ?? '')
    .split(LIST_SEPARATOR)
    .map((item) => item.trim())
    .filter(Boolean);
  return [...new Set(items)];
}

/** The fields of `edited` that differ from `original`, for a PATCH body. */
export function buildPatch<T extends object>(original: object, edited: T): Partial<T> {
  const source = original as Record<string, unknown>;
  return Object.fromEntries(
    Object.entries(edited).filter(([field, value]) => !sameValue(source[field], value)),
  ) as Partial<T>;
}

/** True when two API values are equal (numbers and BigInts compare by value). */
export function sameValue(left: unknown, right: unknown): boolean {
  return canonical(left) === canonical(right);
}

function canonical(value: unknown): string {
  if (typeof value === 'bigint' || typeof value === 'number') return `n:${value}`;
  if (value && typeof value === 'object') {
    const record = value as Record<string, unknown>;
    const entries = Object.keys(record)
      .sort()
      .map((key) => `${key}=${canonical(record[key])}`);
    return `{${entries.join(',')}}`;
  }
  return JSON.stringify(value ?? null);
}
