import {
  ForbiddenError,
  IdbSeqNetworkError,
  InternalError,
  UnauthorizedError,
  type IdbSeqErrorInit,
} from 'idbseq';
import { describe, expect, it, vi } from 'vitest';
import { StartupError, verifyConnection } from '../src/startup.js';

const URL = 'http://127.0.0.1:7878';

function init(code: string, status: number): IdbSeqErrorInit {
  return { message: `${code} message`, code, status, details: null, requestId: null, retryAfterSeconds: null };
}

function failingWith(error: Error) {
  return { listCollections: vi.fn().mockRejectedValue(error) };
}

describe('verifyConnection', () => {
  it('passes when the key can read', async () => {
    const api = { listCollections: vi.fn().mockResolvedValue({ items: [], total: 0, limit: 1, nextCursor: null }) };

    await expect(verifyConnection(api, URL)).resolves.toBeUndefined();
    expect(api.listCollections).toHaveBeenCalledWith({ limit: 1 });
  });

  it('passes for a generate-only key (FORBIDDEN proves the key is valid)', async () => {
    await expect(verifyConnection(failingWith(new ForbiddenError(init('FORBIDDEN', 403))), URL)).resolves.toBeUndefined();
  });

  it('explains an unreachable server', async () => {
    const run = verifyConnection(failingWith(new IdbSeqNetworkError('request failed: fetch failed')), URL);

    await expect(run).rejects.toThrow(StartupError);
    await expect(run).rejects.toThrow(/cannot reach idbseq at http:\/\/127\.0\.0\.1:7878.*IDBSEQ_URL/);
  });

  it('explains a rejected key', async () => {
    const run = verifyConnection(failingWith(new UnauthorizedError(init('UNAUTHORIZED', 401))), URL);

    await expect(run).rejects.toThrow(/rejected IDBSEQ_KEY/);
  });

  it('reports any other failure', async () => {
    const run = verifyConnection(failingWith(new InternalError(init('INTERNAL', 500))), URL);

    await expect(run).rejects.toThrow(/failed the startup check: INTERNAL message/);
  });
});
