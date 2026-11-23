import { describe, expect, it } from 'vitest';
import { ConfigError, parseCommand } from '../src/config.js';

const KEY = 'idb_k7f3a9c2_secret';

describe('parseCommand', () => {
  it('reads the key and URL from the environment', () => {
    const command = parseCommand([], { IDBSEQ_KEY: KEY, IDBSEQ_URL: 'https://ids.example.com' });

    expect(command).toEqual({
      kind: 'serve',
      config: { url: 'https://ids.example.com', apiKey: KEY, allowCreate: false, warnings: [] },
    });
  });

  it('defaults the URL to the local server', () => {
    const command = parseCommand([], { IDBSEQ_KEY: KEY, IDBSEQ_URL: '  ' });

    expect(command).toMatchObject({ config: { url: 'http://127.0.0.1:7878' } });
  });

  it('turns on create_collection with --allow-create', () => {
    expect(parseCommand(['--allow-create'], { IDBSEQ_KEY: KEY })).toMatchObject({ config: { allowCreate: true } });
  });

  it('shows help without needing a key', () => {
    expect(parseCommand(['--help'], {})).toEqual({ kind: 'help' });
  });

  it('refuses to start without a key', () => {
    expect(() => parseCommand([], { IDBSEQ_KEY: '' })).toThrow(ConfigError);
    expect(() => parseCommand([], {})).toThrow(/IDBSEQ_KEY is not set/);
  });

  it('refuses unknown arguments and shows the usage', () => {
    expect(() => parseCommand(['--allow-delete'], { IDBSEQ_KEY: KEY })).toThrow(/Usage: idbseq-mcp/);
  });

  it('refuses a key with characters a key cannot have, without echoing it', () => {
    const secret = 'idb_a_secret\nX-Injected: 1';

    expect(() => parseCommand([], { IDBSEQ_KEY: secret })).toThrow(ConfigError);
    expect(() => parseCommand([], { IDBSEQ_KEY: secret })).not.toThrow(/secret/);
  });

  it.each(['not a url', 'ftp://ids.example.com'])('refuses IDBSEQ_URL %s', (url) => {
    expect(() => parseCommand([], { IDBSEQ_KEY: KEY, IDBSEQ_URL: url })).toThrow(/IDBSEQ_URL must be an http or https URL/);
  });

  it('warns when the credential is not an API key', () => {
    const command = parseCommand([], { IDBSEQ_KEY: 'opaque-admin-session-token' });

    expect(command).toMatchObject({ config: { warnings: [expect.stringMatching(/Admin session tokens/)] } });
  });
});
