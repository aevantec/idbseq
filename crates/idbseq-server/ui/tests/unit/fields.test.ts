import assert from 'node:assert/strict';
import { describe, test } from 'vitest';

import { buildPatch, parseInteger, parseList, sameValue } from '@/lib/fields';

describe('parseInteger', () => {
  test('returns a Number for exact values', () => {
    assert.deepEqual(parseInteger(' 42 '), { value: 42 });
    assert.deepEqual(parseInteger('-7'), { value: -7 });
  });

  test('returns a BigInt beyond 2^53', () => {
    assert.deepEqual(parseInteger('9223372036854775807'), { value: 9223372036854775807n });
  });

  test('rejects text that is not a whole number', () => {
    for (const text of ['', 'abc', '1.5', '1e3', '0x10', null, undefined]) {
      assert.equal(parseInteger(text, { label: 'Start' }).error, 'Start must be a whole number.', String(text));
    }
  });

  test('rejects values outside the signed 64-bit range by default', () => {
    assert.match(String(parseInteger('9223372036854775808').error), /at most 9223372036854775807/);
    assert.match(String(parseInteger('-9223372036854775809').error), /at least -9223372036854775808/);
  });

  test('applies custom bounds', () => {
    assert.equal(parseInteger('0', { label: 'Count', min: 1, max: 10 }).error, 'Count must be at least 1.');
    assert.equal(parseInteger('11', { label: 'Count', min: 1, max: 10 }).error, 'Count must be at most 10.');
    assert.deepEqual(parseInteger('10', { min: 1, max: 10 }), { value: 10 });
  });
});

describe('parseList', () => {
  test('splits on commas, spaces and new lines, without blanks or repeats', () => {
    assert.deepEqual(parseList(' orders, eu.*\n\norders  invoices,'), ['orders', 'eu.*', 'invoices']);
  });

  test('returns an empty list for empty input', () => {
    assert.deepEqual(parseList(''), []);
    assert.deepEqual(parseList(undefined), []);
  });
});

describe('buildPatch', () => {
  const original = { name: 'orders', start: 1, step: 1, max: 9223372036854775807n, protected: false, durability: { mode: 'strict' } };

  test('keeps only changed fields', () => {
    const edited = { start: 1, step: 2, max: 9223372036854775807n, protected: true, durability: { mode: 'strict' } };

    assert.deepEqual(buildPatch(original, edited), { step: 2, protected: true });
  });

  test('compares numbers and BigInts by value', () => {
    assert.deepEqual(buildPatch({ max: 100 }, { max: 100n }), {});
  });

  test('detects a durability change, ignoring key order', () => {
    const toBlock = buildPatch({ durability: { mode: 'strict' } }, { durability: { size: 50, mode: 'block' } });
    const unchanged = buildPatch({ durability: { mode: 'block', size: 50 } }, { durability: { size: 50, mode: 'block' } });

    assert.deepEqual(toBlock, { durability: { size: 50, mode: 'block' } });
    assert.deepEqual(unchanged, {});
  });

  test('does not change its inputs', () => {
    const before = structuredClone(original);

    buildPatch(original, { step: 3 });

    assert.deepEqual(original, before);
  });
});

describe('sameValue', () => {
  test('treats null and undefined as equal, strings as distinct from numbers', () => {
    assert.equal(sameValue(null, undefined), true);
    assert.equal(sameValue('1', 1), false);
    assert.equal(sameValue(['a'], ['a']), true);
  });
});
