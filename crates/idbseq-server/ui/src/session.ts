// The admin session, kept only in memory: never in cookies, localStorage or
// sessionStorage. Reloading or closing the tab signs out.
//
// `idbseqd console <file>` has no accounts: its link carries a token that is
// the admin for as long as that command runs. Then `file` is the file's name.

import { reactive, readonly } from 'vue';

import { createApi } from './lib/api';
import type { SessionDto } from './lib/types';

interface SessionState {
  token: string | null;
  /** Epoch milliseconds, or null when the server's time was unparsable. */
  expiresAt: number | null;
  /** Shown once on the login screen, e.g. "Your session ended." */
  notice: string | null;
  /** The file's name in `idbseqd console`; null on a server. Kept after the session ends. */
  file: string | null;
}

const state = reactive<SessionState>({ token: null, expiresAt: null, notice: null, file: null });

/** Shown when a console link stops working: the command was stopped or restarted. */
export const LOCAL_ENDED =
  'This console link no longer works: idbseqd console was stopped or restarted. Run it again and open the new link.';

export const session = readonly(state);

export const api = createApi({
  getToken: () => state.token,
  onUnauthorized: () => endSession(state.file === null ? 'Your session ended. Sign in again.' : LOCAL_ENDED),
});

function parseExpiry(expiresAt: string): number | null {
  const expires = Date.parse(expiresAt);
  return Number.isNaN(expires) ? null : expires;
}

export function signIn({ token, expires_at: expiresAt }: SessionDto): void {
  state.token = token;
  state.expiresAt = parseExpiry(expiresAt);
  state.notice = null;
  state.file = null;
}

/** `idbseqd console`: the link's token, with no expiry, for the file `file`. */
export function signInLocal(token: string, file: string): void {
  state.token = token;
  state.expiresAt = null;
  state.notice = null;
  state.file = file;
}

/**
 * Keep the session alive on the server and take its new expiry. A reply for a
 * session that has since ended or been replaced is ignored.
 *
 * @throws the API error, e.g. when the server cannot be reached
 */
export async function extendSession(): Promise<void> {
  const token = state.token;
  if (token === null) return;
  const { data } = await api.refresh();
  if (state.token !== token) return;
  state.expiresAt = parseExpiry(data.expires_at);
}

export function endSession(notice: string | null = null): void {
  if (state.token === null) return;
  state.token = null;
  state.expiresAt = null;
  state.notice = notice;
}

/** Revoke the session on the server, then forget it here either way. */
export async function signOut(): Promise<void> {
  try {
    await api.logout();
    endSession('Signed out.');
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    endSession(`Signed out here, but the server could not revoke the session: ${reason}`);
  }
}

/** Milliseconds left in the session, or null when unknown. */
export function timeLeft(now: number = Date.now()): number | null {
  return state.expiresAt === null ? null : state.expiresAt - now;
}

export function takeNotice(): string | null {
  const notice = state.notice;
  state.notice = null;
  return notice;
}
