import { ForbiddenError, IdbSeqNetworkError, UnauthorizedError, type IdbSeqClient } from 'idbseq';

/** idbseqd cannot be used with this configuration; the message is meant for the user. */
export class StartupError extends Error {
  override readonly name = 'StartupError';
}

/**
 * Checks that the server answers and accepts the key before serving tools, so a
 * misconfiguration shows up when the MCP client starts us, not on the first tool call.
 * A key without the `read` scope gets FORBIDDEN here, which still proves it is valid.
 */
export async function verifyConnection(api: Pick<IdbSeqClient, 'listCollections'>, url: string): Promise<void> {
  try {
    await api.listCollections({ limit: 1 });
  } catch (error) {
    if (error instanceof ForbiddenError) {
      return;
    }
    throw startupErrorFor(error, url);
  }
}

function startupErrorFor(error: unknown, url: string): StartupError {
  const detail = error instanceof Error ? error.message : String(error);
  if (error instanceof IdbSeqNetworkError) {
    return new StartupError(`cannot reach idbseq at ${url} (${detail}). Is idbseqd running? Check IDBSEQ_URL.`, {
      cause: error,
    });
  }
  if (error instanceof UnauthorizedError) {
    return new StartupError(`idbseq at ${url} rejected IDBSEQ_KEY (${detail}). The key may be wrong or revoked.`, {
      cause: error,
    });
  }
  return new StartupError(`idbseq at ${url} failed the startup check: ${detail}`, { cause: error });
}
