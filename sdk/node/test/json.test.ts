import { describe, expect, it } from 'vitest';
import { parseJson, parseNumber, stringifyJson } from '../src/json.js';

describe('parseNumber', () => {
  it('returns a number for safe integers', () => {
    expect(parseNumber('42')).toBe(42);
    expect(parseNumber('-7')).toBe(-7);
    expect(parseNumber('9007199254740991')).toBe(9007199254740991);
  });

  it('returns a bigint for integers beyond the safe range', () => {
    expect(parseNumber('9007199254740992')).toBe(9007199254740992n);
    expect(parseNumber('9223372036854775807')).toBe(9223372036854775807n);
    expect(parseNumber('-9223372036854775808')).toBe(-9223372036854775808n);
  });

  it('returns a number for non-integers', () => {
    expect(parseNumber('1.5')).toBe(1.5);
    expect(parseNumber('1e400')).toBe(Infinity);
  });
});

describe('parseJson', () => {
  it('keeps the exact value of a 64-bit ID', () => {
    // Arrange
    const text = '{"data":{"id":9223372036854775807,"small":1043}}';

    // Act
    const value = parseJson(text) as { data: { id: bigint; small: number } };

    // Assert
    expect(value.data.id).toBe(9223372036854775807n);
    expect(value.data.small).toBe(1043);
  });

  it('throws a SyntaxError on invalid JSON', () => {
    expect(() => parseJson('{"id":')).toThrow(SyntaxError);
  });
});

describe('stringifyJson', () => {
  it('writes bigints as plain JSON numbers', () => {
    expect(stringifyJson({ value: 9223372036854775807n, n: 1 })).toBe('{"value":9223372036854775807,"n":1}');
  });

  it('round-trips through parseJson without losing precision', () => {
    const original = { first: 9007199254740993n, last: 12 };
    expect(parseJson(stringifyJson(original))).toEqual(original);
  });

  it('rejects values that have no JSON form', () => {
    expect(() => stringifyJson(undefined)).toThrow(TypeError);
  });
});
