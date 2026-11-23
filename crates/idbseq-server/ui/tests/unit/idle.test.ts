import { describe, expect, test } from 'vitest';

import { REFRESH_EVERY_MS, WARN_BEFORE_MS, nextIdleAction, type IdleState } from '@/lib/idle';

const MIN = 60_000;
const SIGNED_IN = 1_000_000;
const base: IdleState = {
  now: SIGNED_IN,
  expiresAt: SIGNED_IN + 15 * MIN,
  lastActivityAt: SIGNED_IN,
  lastRefreshAt: SIGNED_IN,
  isWarning: false,
  isAtLimit: false,
};

describe('nextIdleAction', () => {
  test('does nothing right after signing in, or without a known expiry', () => {
    expect(nextIdleAction(base)).toBe('none');
    expect(nextIdleAction({ ...base, expiresAt: null, now: base.now + 99 * MIN })).toBe('none');
  });

  test('keeps an active session alive, at most once a minute', () => {
    const active = { ...base, lastActivityAt: SIGNED_IN + 30_000 };

    expect(nextIdleAction({ ...active, now: SIGNED_IN + 30_000 })).toBe('none');
    expect(nextIdleAction({ ...active, now: SIGNED_IN + REFRESH_EVERY_MS })).toBe('refresh');
  });

  test('without activity it warns a minute before the end, then expires', () => {
    const expiresAt = base.expiresAt ?? 0;

    expect(nextIdleAction({ ...base, now: expiresAt - WARN_BEFORE_MS - 1 })).toBe('none');
    expect(nextIdleAction({ ...base, now: expiresAt - WARN_BEFORE_MS })).toBe('warn');
    expect(nextIdleAction({ ...base, now: expiresAt })).toBe('expire');
  });

  test('activity just before the warning refreshes instead of warning', () => {
    const now = (base.expiresAt ?? 0) - 30_000;
    expect(nextIdleAction({ ...base, now, lastActivityAt: now - 1000 })).toBe('refresh');
  });

  test('while the prompt is open only a choice counts, but the end still comes', () => {
    const warning = { ...base, isWarning: true, lastActivityAt: SIGNED_IN + 14 * MIN };

    expect(nextIdleAction({ ...warning, now: SIGNED_IN + 14.5 * MIN })).toBe('none');
    expect(nextIdleAction({ ...warning, now: SIGNED_IN + 15 * MIN })).toBe('expire');
  });

  test('at the maximum session length it stops refreshing and warns', () => {
    const now = (base.expiresAt ?? 0) - 30_000;
    const atLimit = { ...base, now, lastActivityAt: now - 1000, isAtLimit: true };

    expect(nextIdleAction(atLimit)).toBe('warn');
    expect(nextIdleAction({ ...atLimit, now: now - 10 * MIN })).toBe('none');
  });
});
