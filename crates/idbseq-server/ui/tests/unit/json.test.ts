import assert from 'node:assert/strict';
import { describe, test } from 'vitest';

import { parseJson, stringifyJson } from '@/lib/json';

describe('parseJson', () => {
  test('keeps safe integers as numbers', () => {
    assert.deepEqual(parseJson('{"id":1004,"neg":-5,"ratio":0.5}'), { id: 1004, neg: -5, ratio: 0.5 });
  });

  test('parses integers beyond 2^53 as exact BigInt values', () => {
    const parsed = parseJson('{"max":9223372036854775807,"min":-9223372036854775808}') as Record<string, unknown>;

    assert.equal(parsed.max, 9223372036854775807n);
    assert.equal(parsed.min, -9223372036854775808n);
  });

  test('leaves digits inside strings alone, including escaped quotes', () => {
    const parsed = parseJson('{"name":"a \\"9223372036854775807\\" b","n":9007199254740993}') as Record<string, unknown>;

    assert.equal(parsed.name, 'a "9223372036854775807" b');
    assert.equal(parsed.n, 9007199254740993n);
  });

  test('keeps large floats and exponents as numbers', () => {
    assert.deepEqual(parseJson('[1e30, 12345678901234567890.5]'), [1e30, 12345678901234567890.5]);
  });

  test('handles arrays, nesting and null', () => {
    assert.deepEqual(parseJson('{"a":[null,{"b":18446744073709551615}]}'), {
      a: [null, { b: 18446744073709551615n }],
    });
  });

  test('does not turn a crafted placeholder string with non-digits into a BigInt', () => {
    assert.equal(parseJson('"\\u0000big:abc"'), '\u0000big:abc');
  });

  test('throws on invalid JSON', () => {
    assert.throws(() => parseJson('{"a":'), SyntaxError);
  });

  test('copes with an unterminated string', () => {
    assert.throws(() => parseJson('"abc'), SyntaxError);
  });
});

describe('stringifyJson', () => {
  test('writes BigInt values as bare integers', () => {
    assert.equal(stringifyJson({ start: 9223372036854775807n, step: 1 }), '{"start":9223372036854775807,"step":1}');
  });

  test('round-trips with parseJson', () => {
    const value = { a: -9223372036854775808n, b: [1, 'x', null, true] };

    assert.deepEqual(parseJson(String(stringifyJson(value))), value);
  });

  test('returns undefined for undefined, like JSON.stringify', () => {
    assert.equal(stringifyJson(undefined), undefined);
  });
});

describe('stringifyJson with indent', () => {
  test('pretty-prints and still writes BigInt values as bare integers', () => {
    assert.equal(stringifyJson({ id: 9223372036854775807n }, 2), '{\n  "id": 9223372036854775807\n}');
  });
});
