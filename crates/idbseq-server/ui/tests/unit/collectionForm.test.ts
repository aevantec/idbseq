import { describe, expect, test } from 'vitest';

import { readCollectionForm, type CollectionFormValues } from '@/lib/collectionForm';

const BLANK: CollectionFormValues = { name: '', start: '', step: '', max: '', mode: 'strict', blockSize: '100', isProtected: false };

describe('readCollectionForm', () => {
  test('on create, leaves empty optional fields to the server defaults', () => {
    const { value, errors } = readCollectionForm({ ...BLANK, name: ' orders ' }, { isCreate: true });

    expect(errors).toEqual([]);
    expect(value).toEqual({ name: 'orders', durability: { mode: 'strict' }, protected: false });
  });

  test('requires a name on create only', () => {
    expect(readCollectionForm(BLANK, { isCreate: true }).errors).toContain('Name is required.');
    expect(readCollectionForm({ ...BLANK, start: '1', step: '1', max: '9' }, { isCreate: false }).errors).toEqual([]);
  });

  test('accepts the server bounds: a u64 step and a u32 block size', () => {
    const values = { ...BLANK, start: '1', step: '18446744073709551615', max: '9223372036854775807', mode: 'block' as const, blockSize: '4294967295' };

    const { value, errors } = readCollectionForm(values, { isCreate: false });

    expect(errors).toEqual([]);
    expect(value.step).toBe(18446744073709551615n);
    expect(value.max).toBe(9223372036854775807n);
    expect(value.durability).toEqual({ mode: 'block', size: 4294967295 });
  });

  test('rejects values outside those bounds', () => {
    const values = { ...BLANK, start: '1', step: '0', max: '9223372036854775808', mode: 'block' as const, blockSize: '4294967296' };

    const { errors } = readCollectionForm(values, { isCreate: false });

    expect(errors).toEqual([
      'Step must be at least 1.',
      'Max must be at most 9223372036854775807.',
      'Block size must be at most 4294967295.',
    ]);
  });

  test('rejects text that is not a number', () => {
    expect(readCollectionForm({ ...BLANK, name: 'x', start: 'ten' }, { isCreate: true }).errors).toEqual(['Start must be a whole number.']);
  });
});
