import assert from 'node:assert/strict';
import { describe, test } from 'vitest';

import {
  formatBatch,
  formatCountdown,
  formatDurability,
  formatId,
  formatRatio,
  formatTime,
  formatValue,
  remainingRatio,
  summarizeChange,
} from '@/lib/format';

describe('formatId', () => {
  test('shows IDs exactly and a dash when there is none', () => {
    assert.equal(formatId(9223372036854775807n), '9223372036854775807');
    assert.equal(formatId(0), '0');
    assert.equal(formatId(null), '—');
    assert.equal(formatId(undefined), '—');
  });
});

describe('remainingRatio', () => {
  test('is 1 for an unused collection', () => {
    assert.equal(remainingRatio({ start: 1, max: 100, next_id: 1 }), 1);
  });

  test('is the share of IDs left', () => {
    assert.equal(remainingRatio({ start: 1, max: 100, next_id: 51 }), 0.5);
  });

  test('is 0 when exhausted or when next_id is missing', () => {
    assert.equal(remainingRatio({ start: 1, max: 100, next_id: null }), 0);
    assert.equal(remainingRatio({ start: 1, max: 100, next_id: 101 }), 0);
    assert.equal(remainingRatio({ start: 5, max: 1, next_id: 5 }), 0);
  });

  test('works across the full 64-bit range without rounding to 1', () => {
    const ratio = remainingRatio({ start: 1, max: 9223372036854775807n, next_id: 4611686018427387904n });

    assert.ok(Math.abs(ratio - 0.5) < 1e-6, String(ratio));
  });
});

describe('formatRatio', () => {
  test('formats a percentage with one decimal', () => {
    assert.equal(formatRatio(1), '100.0%');
    assert.equal(formatRatio(0.4567), '45.7%');
    assert.equal(formatRatio(0), '0.0%');
  });

  test('marks tiny non-zero shares instead of rounding them to zero', () => {
    assert.equal(formatRatio(0.0001), '<0.1%');
  });
});

describe('formatDurability', () => {
  test('names the mode and the block size', () => {
    assert.equal(formatDurability({ mode: 'strict' }), 'strict');
    assert.equal(formatDurability({ mode: 'block', size: 100 }), 'block (100)');
    assert.equal(formatDurability(undefined), '—');
  });
});

describe('formatTime', () => {
  test('formats valid times and passes through anything else', () => {
    assert.equal(formatTime('2026-11-17T10:00:00Z', 'en-US').includes('2026'), true);
    assert.equal(formatTime('not a time'), 'not a time');
    assert.equal(formatTime(null), '—');
  });
});

describe('formatCountdown', () => {
  test('formats minutes and seconds, never negative', () => {
    assert.equal(formatCountdown(14 * 60_000 + 32_000), '14:32');
    assert.equal(formatCountdown(500), '0:01');
    assert.equal(formatCountdown(-5000), '0:00');
  });
});

describe('formatBatch', () => {
  test('describes one ID or a range', () => {
    assert.equal(formatBatch({ first: 7, last: 7, count: 1 }), 'ID 7');
    assert.equal(formatBatch({ first: 8, last: 10, count: 3 }), '3 IDs: 8 … 10');
  });
});

describe('formatValue', () => {
  test('shows strings as-is and other values as JSON', () => {
    assert.equal(formatValue('orders'), 'orders');
    assert.equal(formatValue({ mode: 'block', size: 5 }), '{"mode":"block","size":5}');
    assert.equal(formatValue(9223372036854775807n), '9223372036854775807');
    assert.equal(formatValue(null), '—');
  });
});

describe('summarizeChange', () => {
  test('lists only the fields that changed', () => {
    const rows = summarizeChange({ step: 1, max: 100, name: 'a' }, { step: 2, max: 100, name: 'a' });

    assert.deepEqual(rows, [{ field: 'step', kind: 'changed', before: '1', after: '2' }]);
  });

  test('lists every field of a creation', () => {
    const rows = summarizeChange(null, { name: 'orders', scopes: ['read'] });

    assert.deepEqual(rows, [
      { field: 'name', kind: 'added', before: '—', after: 'orders' },
      { field: 'scopes', kind: 'added', before: '—', after: '["read"]' },
    ]);
  });

  test('marks the fields of a deletion as removed', () => {
    assert.deepEqual(summarizeChange({ name: 'k' }, null), [{ field: 'name', kind: 'removed', before: 'k', after: '—' }]);
  });

  test('wraps scalar values', () => {
    assert.deepEqual(summarizeChange(5, 7), [{ field: 'value', kind: 'changed', before: '5', after: '7' }]);
    assert.deepEqual(summarizeChange(undefined, undefined), []);
  });
});
