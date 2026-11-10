import { createServer, type IncomingHttpHeaders, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';

export interface RecordedRequest {
  readonly method: string;
  readonly path: string;
  readonly query: URLSearchParams;
  readonly headers: IncomingHttpHeaders;
  readonly body: string;
}

export interface MockReply {
  readonly status?: number;
  /** Raw response body, sent as-is. */
  readonly raw?: string;
  /** Response body, serialised with JSON.stringify. */
  readonly json?: unknown;
  readonly headers?: Readonly<Record<string, string>>;
  /** Close the socket without answering, to simulate a network failure. */
  readonly drop?: boolean;
  /** Wait this long before answering. */
  readonly delayMs?: number;
}

export function ok(data: unknown, meta?: unknown): MockReply {
  const envelope = meta === undefined
    ? { success: true, data, error: null }
    : { success: true, data, error: null, meta };
  return { status: 200, json: envelope };
}

export function okRaw(dataJson: string, status = 200): MockReply {
  return { status, raw: `{"success":true,"data":${dataJson},"error":null}` };
}

export function fail(
  status: number,
  code: string,
  message = 'failed',
  details: unknown = null,
  headers: Readonly<Record<string, string>> = {},
): MockReply {
  return {
    status,
    json: { success: false, data: null, error: { code, message, details } },
    headers,
  };
}

const sleep = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** A local HTTP server that answers with queued replies and records every request. */
export class MockServer {
  readonly requests: RecordedRequest[] = [];
  private readonly replies: MockReply[] = [];

  private constructor(private readonly server: Server) {}

  static async start(): Promise<MockServer> {
    const server = createServer();
    const mock = new MockServer(server);
    server.on('request', (req, res) => {
      const chunks: Buffer[] = [];
      req.on('data', (chunk: Buffer) => chunks.push(chunk));
      req.on('end', () => {
        const url = new URL(req.url ?? '/', 'http://localhost');
        mock.requests.push({
          method: req.method ?? 'GET',
          path: url.pathname,
          query: url.searchParams,
          headers: req.headers,
          body: Buffer.concat(chunks).toString('utf8'),
        });
        const reply = mock.replies.shift() ?? fail(599, 'NO_REPLY_QUEUED', 'the test queued no reply');
        void mock.answer(reply, res);
      });
    });
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    return mock;
  }

  get url(): string {
    const { port } = this.server.address() as AddressInfo;
    return `http://127.0.0.1:${port}`;
  }

  get lastRequest(): RecordedRequest {
    const last = this.requests.at(-1);
    if (last === undefined) {
      throw new Error('no request was received');
    }
    return last;
  }

  reply(...replies: MockReply[]): void {
    this.replies.push(...replies);
  }

  reset(): void {
    this.requests.length = 0;
    this.replies.length = 0;
  }

  async close(): Promise<void> {
    this.server.closeAllConnections();
    await new Promise<void>((resolve) => this.server.close(() => resolve()));
  }

  private async answer(
    reply: MockReply,
    res: import('node:http').ServerResponse,
  ): Promise<void> {
    if (reply.delayMs !== undefined) {
      await sleep(reply.delayMs);
    }
    if (reply.drop === true) {
      res.socket?.destroy();
      return;
    }
    const body = reply.raw ?? (reply.json === undefined ? '' : JSON.stringify(reply.json));
    res.writeHead(reply.status ?? 200, {
      'content-type': 'application/json',
      'x-request-id': 'req-test-1',
      ...reply.headers,
    });
    res.end(body);
  }
}
