// Idle sign-out. While the admin is active the console keeps the session alive
// (POST /v1/auth/refresh), so it ends only after a full session TTL without
// activity. A minute before that, the console asks whether to stay signed in.

/** How long before the end the "Still there?" prompt appears. */
export const WARN_BEFORE_MS = 60_000;
/** The most often an active session is refreshed. */
export const REFRESH_EVERY_MS = 60_000;

export type IdleAction = 'none' | 'refresh' | 'warn' | 'expire';

export interface IdleState {
  now: number;
  /** Epoch ms, or null when unknown (then nothing is decided here). */
  expiresAt: number | null;
  lastActivityAt: number;
  /** When the session was last refreshed, or a refresh last attempted. */
  lastRefreshAt: number;
  /** The prompt is open: only the admin's choice extends the session. */
  isWarning: boolean;
  /** The server would not extend it further (12 hours since sign-in). */
  isAtLimit: boolean;
}

/** What the console should do now. */
export function nextIdleAction(state: IdleState): IdleAction {
  const { now, expiresAt, lastActivityAt, lastRefreshAt, isWarning, isAtLimit } = state;
  if (expiresAt === null) return 'none';
  if (now >= expiresAt) return 'expire';
  if (isWarning) return 'none';
  const isActive = lastActivityAt > lastRefreshAt;
  if (!isAtLimit && isActive && now - lastRefreshAt >= REFRESH_EVERY_MS) return 'refresh';
  if (expiresAt - now <= WARN_BEFORE_MS) return 'warn';
  return 'none';
}
