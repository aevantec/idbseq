import { parseArgs } from 'node:util';
import { DEFAULT_URL } from 'idbseq';

/** Opt-in flag that adds the `create_collection` tool. */
export const ALLOW_CREATE_FLAG = 'allow-create';

/** `idb_<id>_<secret>`: the shape of an API key created by the admin. */
const API_KEY_PATTERN = /^idb_[^_]+_.+$/;

/** A bearer token must be visible ASCII; anything else would also break the HTTP header. */
const TOKEN_PATTERN = /^[\x21-\x7e]+$/;

export const USAGE = `Usage: idbseq-mcp [--allow-create]

MCP server (stdio) that hands out IDs from an idbseq server.

Environment:
  IDBSEQ_KEY   API key, required. Give it only the generate and read scopes,
               and a collection allow-list.
  IDBSEQ_URL   Server base URL, default ${DEFAULT_URL}

Options:
  --allow-create   Also offer the create_collection tool (the key needs the
                   manage scope). Delete and reset are never offered.
  --help           Show this help.`;

export interface ServerConfig {
  readonly url: string;
  readonly apiKey: string;
  readonly allowCreate: boolean;
  /** Problems worth telling the user about that do not stop the server. */
  readonly warnings: readonly string[];
}

export type ParsedCommand = { readonly kind: 'help' } | { readonly kind: 'serve'; readonly config: ServerConfig };

/** Bad arguments or environment; the message is meant for the user as is. */
export class ConfigError extends Error {
  override readonly name = 'ConfigError';
}

function parseFlags(argv: readonly string[]): { readonly allowCreate: boolean; readonly help: boolean } {
  try {
    const { values } = parseArgs({
      args: [...argv],
      options: {
        [ALLOW_CREATE_FLAG]: { type: 'boolean', default: false },
        help: { type: 'boolean', short: 'h', default: false },
      },
      strict: true,
      allowPositionals: false,
    });
    return { allowCreate: values[ALLOW_CREATE_FLAG] === true, help: values.help === true };
  } catch (cause) {
    const detail = cause instanceof Error ? cause.message : String(cause);
    throw new ConfigError(`${detail}\n\n${USAGE}`, { cause });
  }
}

function keyWarnings(apiKey: string): readonly string[] {
  if (API_KEY_PATTERN.test(apiKey)) {
    return [];
  }
  return [
    'IDBSEQ_KEY does not look like an API key (idb_<id>_<secret>). Admin session tokens ' +
      'expire after 15 minutes and may do everything; create a key with ' +
      '`idbseq keys create <name> --scope generate --scope read --collection <name>` instead.',
  ];
}

function parseUrl(raw: string): string {
  const url = URL.canParse(raw) ? new URL(raw) : null;
  if (url === null || (url.protocol !== 'http:' && url.protocol !== 'https:')) {
    throw new ConfigError(`IDBSEQ_URL must be an http or https URL, e.g. ${DEFAULT_URL} (got "${raw}").`);
  }
  return raw;
}

/** Reads the command line and environment. Throws `ConfigError` with a user-facing message. */
export function parseCommand(argv: readonly string[], env: Readonly<Record<string, string | undefined>>): ParsedCommand {
  const flags = parseFlags(argv);
  if (flags.help) {
    return { kind: 'help' };
  }
  const apiKey = env.IDBSEQ_KEY?.trim() ?? '';
  if (apiKey === '') {
    throw new ConfigError(
      'IDBSEQ_KEY is not set. Every idbseq call needs an API key: create one with ' +
        '`idbseq keys create <name> --scope generate --scope read` and pass it in the ' +
        "MCP server's environment.",
    );
  }
  if (!TOKEN_PATTERN.test(apiKey)) {
    // Never echo the value: it is a secret, and stderr often ends up in log files.
    throw new ConfigError('IDBSEQ_KEY contains spaces or other characters a key cannot have. Copy the key again.');
  }
  const url = parseUrl(env.IDBSEQ_URL?.trim() || DEFAULT_URL);
  const config = { url, apiKey, allowCreate: flags.allowCreate, warnings: keyWarnings(apiKey) };
  return { kind: 'serve', config };
}
