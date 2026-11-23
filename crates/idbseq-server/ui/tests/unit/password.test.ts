import { describe, expect, test } from 'vitest';

import { MIN_PASSWORD_LEN, validateNewPassword, validatePasswordChange } from '@/lib/password';

const GOOD = { current: 'the old password', next: 'a brand new password', confirm: 'a brand new password' };

describe('validatePasswordChange', () => {
  test('accepts a valid change', () => {
    expect(validatePasswordChange(GOOD)).toEqual([]);
  });

  test('requires the current password', () => {
    expect(validatePasswordChange({ ...GOOD, current: '' })).toEqual(['Enter your current password.']);
  });

  test(`requires at least ${MIN_PASSWORD_LEN} characters, counted like the server (code points)`, () => {
    expect(validatePasswordChange({ ...GOOD, next: 'short', confirm: 'short' })).toEqual([
      'The new password must be at least 8 characters.',
    ]);
    // 8 emoji are 16 UTF-16 units but 8 characters: accepted, as by the server.
    const emoji = '🔑'.repeat(8);
    expect(validatePasswordChange({ ...GOOD, next: emoji, confirm: emoji })).toEqual([]);
    const seven = '🔑'.repeat(7);
    expect(validatePasswordChange({ ...GOOD, next: seven, confirm: seven })).toHaveLength(1);
  });

  test('requires the confirmation to match', () => {
    expect(validatePasswordChange({ ...GOOD, confirm: 'something else entirely' })).toEqual([
      'The new passwords do not match.',
    ]);
  });

  test('requires a password different from the current one', () => {
    const same = { current: GOOD.next, next: GOOD.next, confirm: GOOD.next };
    expect(validatePasswordChange(same)).toEqual(['The new password must differ from the current one.']);
  });

  test('reports every problem at once', () => {
    expect(validatePasswordChange({ current: '', next: 'short', confirm: 'other' })).toHaveLength(3);
  });
});

describe('validateNewPassword', () => {
  test('accepts a long enough, confirmed password', () => {
    expect(validateNewPassword('a long enough password', 'a long enough password')).toEqual([]);
  });

  test('reports a short password and a mismatch together', () => {
    expect(validateNewPassword('short', 'other')).toEqual([
      'The new password must be at least 8 characters.',
      'The new passwords do not match.',
    ]);
  });
});
