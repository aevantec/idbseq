import { afterEach, describe, expect, test, vi } from 'vitest';

import { api, endSession, extendSession, session, signIn, signInLocal, signOut, takeNotice, timeLeft } from '@/session';

const DTO = { token: 'tok', expires_at: '2026-11-17T10:15:00Z' };

afterEach(() => {
  endSession();
  takeNotice();
  vi.restoreAllMocks();
});

describe('session', () => {
  test('keeps the token and expiry in memory, nowhere else', () => {
    signIn(DTO);

    expect(session.token).toBe('tok');
    expect(session.expiresAt).toBe(Date.parse(DTO.expires_at));
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
    expect(document.cookie).toBe('');
  });

  test('counts down to the expiry, and knows nothing when the time is unparsable', () => {
    signIn(DTO);
    expect(timeLeft(Date.parse(DTO.expires_at) - 60_000)).toBe(60_000);

    signIn({ token: 't2', expires_at: 'soon' });
    expect(timeLeft()).toBeNull();
  });

  test('ending the session leaves a notice for the login screen, read once', () => {
    signIn(DTO);

    endSession('Your session expired.');

    expect(session.token).toBeNull();
    expect(takeNotice()).toBe('Your session expired.');
    expect(takeNotice()).toBeNull();
  });

  test('ending an already ended session changes nothing', () => {
    endSession('first');
    expect(takeNotice()).toBeNull();
  });

  test('signing out revokes the session on the server', async () => {
    signIn(DTO);
    const logout = vi.spyOn(api, 'logout').mockResolvedValue({ data: null, meta: null });

    await signOut();

    expect(logout).toHaveBeenCalledOnce();
    expect(session.token).toBeNull();
    expect(takeNotice()).toBe('Signed out.');
  });

  test('signing out still forgets the session when the server cannot be reached', async () => {
    signIn(DTO);
    vi.spyOn(api, 'logout').mockRejectedValue(new Error('offline'));

    await signOut();

    expect(session.token).toBeNull();
    expect(takeNotice()).toMatch(/could not revoke the session: offline/);
  });

  test('extending the session moves its expiry and keeps the token', async () => {
    signIn(DTO);
    const later = '2026-11-17T10:30:00Z';
    const refresh = vi.spyOn(api, 'refresh').mockResolvedValue({ data: { expires_at: later }, meta: null });

    await extendSession();

    expect(refresh).toHaveBeenCalledOnce();
    expect(session.token).toBe('tok');
    expect(session.expiresAt).toBe(Date.parse(later));
  });

  test('a late refresh does not touch a session that ended or changed meanwhile', async () => {
    signIn(DTO);
    let answer: (value: { data: { expires_at: string }; meta: null }) => void = () => {};
    vi.spyOn(api, 'refresh').mockReturnValue(new Promise((resolve) => (answer = resolve)));

    const pending = extendSession();
    signIn({ token: 'other', expires_at: '2026-11-17T11:00:00Z' });
    answer({ data: { expires_at: '2026-11-17T10:30:00Z' }, meta: null });
    await pending;

    expect(session.token).toBe('other');
    expect(session.expiresAt).toBe(Date.parse('2026-11-17T11:00:00Z'));
  });
});

describe('a console on a file', () => {
  test('signs in with the link token, with no expiry, and remembers the file', () => {
    signInLocal('link-token', 'app.idbseq');

    expect(session.token).toBe('link-token');
    expect(session.file).toBe('app.idbseq');
    expect(timeLeft()).toBeNull();

    endSession();
    expect(session.file).toBe('app.idbseq'); // kept, so the console can say the link ended

    signIn(DTO);
    expect(session.file).toBeNull();
  });
});
