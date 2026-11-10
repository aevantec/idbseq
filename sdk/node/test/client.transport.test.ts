import { afterAll, beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  IdbseqClient,
  IdbseqNetworkError,
  UnexpectedResponseError,
  UnknownCodeError,
} from '../src/index.js';
import { MockServer, ok } from './helpers/mock-server.js';

let server: MockServer;

beforeAll(async () => {
  server = await MockServer.start();
});

afterAll(async () => {
  await server.close();
});

beforeEach(() => {
  server.reset();
});

const noRetry = (): IdbseqClient => new IdbseqClient({ url: server.url, apiKey: 'k', retry: false });

describe('client options', () => {
  it('defaults to the local server URL', async () => {
    // Arrange
    const seen: string[] = [];
    const fakeFetch: typeof fetch = async (input) => {
      seen.push(String(input));
      return new Response('{"success":true,"data":{"id":1},"error":null}', { status: 200 });
    };

    // Act
    await new IdbseqClient({ fetch: fakeFetch }).generateId('orders');

    // Assert
    expect(seen).toEqual(['http://127.0.0.1:7878/v1/collections/orders/next']);
  });

  it('keeps a path prefix in the base URL', async () => {
    const seen: string[] = [];
    const fakeFetch: typeof fetch = async (input) => {
      seen.push(String(input));
      return new Response('{"success":true,"data":{"last_id":null,"next_id":1},"error":null}');
    };

    await new IdbseqClient({ url: 'https://ids.example.com/idbseq/', fetch: fakeFetch }).lastId('a');

    expect(seen).toEqual(['https://ids.example.com/idbseq/v1/collections/a/last']);
  });

  it.each(['not a url', 'ftp://example.com'])('rejects the invalid URL %s', (url) => {
    expect(() => new IdbseqClient({ url })).toThrow(TypeError);
  });

  it.each([0, -1, Number.POSITIVE_INFINITY])('rejects the timeout %s', (timeoutMs) => {
    expect(() => new IdbseqClient({ timeoutMs })).toThrow(RangeError);
  });
});

describe('response handling', () => {
  it('maps an unknown error code to UnknownCodeError with the HTTP status', async () => {
    server.reply({
      status: 418,
      json: { success: false, data: null, error: { code: 'TEAPOT', message: 'short and stout' } },
    });

    const error = await noRetry().getCollection('a').catch((e: unknown) => e);

    expect(error).toBeInstanceOf(UnknownCodeError);
    expect((error as UnknownCodeError).status).toBe(418);
    expect((error as UnknownCodeError).details).toBeNull();
  });

  it('reports a non-JSON body as UnexpectedResponseError', async () => {
    server.reply({ status: 502, raw: '<html>Bad Gateway</html>' });

    const error = await noRetry().getCollection('a').catch((e: unknown) => e);

    expect(error).toBeInstanceOf(UnexpectedResponseError);
    expect((error as UnexpectedResponseError).status).toBe(502);
    expect((error as UnexpectedResponseError).code).toBe('UNEXPECTED_RESPONSE');
  });

  it('reports a success envelope with an error status as UnexpectedResponseError', async () => {
    server.reply({ ...ok({ id: 1 }), status: 500 });

    await expect(noRetry().generateId('a')).rejects.toBeInstanceOf(UnexpectedResponseError);
  });

  it('reports data of the wrong shape as UnexpectedResponseError', async () => {
    server.reply(ok({ id: 'one' }));

    const error = await noRetry().generateId('a').catch((e: unknown) => e);

    expect(error).toBeInstanceOf(UnexpectedResponseError);
    expect((error as Error).message).toContain('id');
  });

  it('reports a missing envelope error object as UnexpectedResponseError', async () => {
    server.reply({ status: 400, json: { success: false, data: null, error: null } });

    await expect(noRetry().getCollection('a')).rejects.toBeInstanceOf(UnexpectedResponseError);
  });

  it('parses an HTTP-date Retry-After', async () => {
    const inTenSeconds = new Date(Date.now() + 10_000).toUTCString();
    server.reply({
      status: 429,
      json: { success: false, data: null, error: { code: 'RATE_LIMITED', message: 'slow' } },
      headers: { 'retry-after': inTenSeconds },
    });

    const error = await noRetry().getCollection('a').catch((e: unknown) => e);

    expect((error as UnknownCodeError).retryAfterSeconds).toBeGreaterThanOrEqual(8);
    expect((error as UnknownCodeError).retryAfterSeconds).toBeLessThanOrEqual(10);
  });

  it('ignores a malformed Retry-After', async () => {
    server.reply({
      status: 429,
      json: { success: false, data: null, error: { code: 'RATE_LIMITED', message: 'slow' } },
      headers: { 'retry-after': 'soon' },
    });

    const error = await noRetry().getCollection('a').catch((e: unknown) => e);

    expect((error as UnknownCodeError).retryAfterSeconds).toBeNull();
  });

  it('times out slow requests with IdbseqNetworkError', async () => {
    // Arrange
    const client = new IdbseqClient({ url: server.url, timeoutMs: 50, retry: false });
    server.reply({ ...ok({ id: 1 }), delayMs: 500 });

    // Act
    const error = await client.generateId('a').catch((e: unknown) => e);

    // Assert
    expect(error).toBeInstanceOf(IdbseqNetworkError);
    expect((error as IdbseqNetworkError).message).toContain('timed out');
    expect((error as IdbseqNetworkError).code).toBe('NETWORK_ERROR');
  });
});
