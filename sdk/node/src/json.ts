import { parse, stringify } from 'lossless-json';

const INTEGER_LITERAL = /^-?\d+$/;

/**
 * Converts a JSON number literal: a `number` when it is a safe integer or not an
 * integer at all, a `bigint` for integers beyond `Number.MAX_SAFE_INTEGER`.
 */
export function parseNumber(literal: string): number | bigint {
  const value = Number(literal);
  if (Number.isSafeInteger(value) || !INTEGER_LITERAL.test(literal)) {
    return value;
  }
  return BigInt(literal);
}

/** Parses JSON without losing precision on 64-bit integers. */
export function parseJson(text: string): unknown {
  return parse(text, null, parseNumber);
}

/** Serialises a value to JSON, writing bigints as plain JSON numbers. */
export function stringifyJson(value: unknown): string {
  const text = stringify(value);
  if (text === undefined) {
    throw new TypeError('value has no JSON representation');
  }
  return text;
}
