// The collection page's API preview: for each operation,
// the endpoint, who may call it, ready-to-paste Node.js SDK code, and the
// exact response this collection would give right now.

import { stringifyJson } from './json';
import type { Collection, Id } from './types';

export type HttpMethod = 'GET' | 'POST' | 'PATCH' | 'DELETE';

export interface ApiOperation {
  id: string;
  title: string;
  summary: string;
  method: HttpMethod;
  path: string;
  /** Who may call it: the API key scope, or the admin. */
  access: string;
  /** Node.js SDK code, client setup included. */
  code: string;
  /** The JSON response, pretty-printed. */
  response: string;
}

/** IDs per batch in the example. */
const BATCH_SIZE = 10n;
/** How far the reset example skips ahead, in steps. */
const RESET_SKIP = 1000n;

const ESCAPES: Readonly<Record<string, string>> = {
  '\\': '\\\\',
  "'": "\\'",
  '\n': '\\n',
  '\r': '\\r',
  '\u2028': '\\u2028',
  '\u2029': '\\u2029',
};

/** A single-quoted JavaScript string literal (a file name may hold a newline). */
export function jsString(value: string): string {
  return `'${value.replace(/[\\'\n\r\u2028\u2029]/g, (char) => ESCAPES[char] ?? char)}'`;
}

/** An ID as a JavaScript literal: a BigInt literal beyond 2^53. */
export function jsId(id: Id): string {
  return Number.isSafeInteger(Number(id)) ? String(id) : `${id}n`;
}

function toId(value: bigint): Id {
  return Number.isSafeInteger(Number(value)) ? Number(value) : value;
}

function ok(data: unknown): string {
  return stringifyJson({ success: true, data, error: null }, 2) ?? '';
}

function exhausted(name: string): string {
  const error = { code: 'SEQUENCE_EXHAUSTED', message: `collection \`${name}\` is exhausted` };
  return stringifyJson({ success: false, data: null, error }, 2) ?? '';
}

type Access = 'read' | 'generate' | 'manage';

function describeAccess(access: Access, isProtected: boolean): string {
  if (access === 'manage' && isProtected) return 'Only an admin session: this collection is protected.';
  return `An API key with the ${access} scope, or an admin session.`;
}

function setup(url: string, asAdmin: boolean): string {
  const client = asAdmin
    ? [
        `const ids = new IdbSeqClient({ url: ${jsString(url)} });`,
        '// Protected collection: only the admin may change it.',
        "await ids.login('admin', process.env.IDBSEQ_ADMIN_PASSWORD ?? '');",
      ]
    : ['const ids = new IdbSeqClient({', `  url: ${jsString(url)},`, '  apiKey: process.env.IDBSEQ_KEY,', '});'];
  return ["import { IdbSeqClient } from 'idbseq';", '', ...client].join('\n');
}

interface OperationSpec extends Omit<ApiOperation, 'access' | 'code'> {
  access: Access;
  call: string;
}

/** Embedded: the same calls on the file, with no server or key. */
function setupEmbedded(file: string): string {
  return ["import { IdbSeqDatabase } from 'idbseq';", '', `const ids = await IdbSeqDatabase.open(${jsString(file)});`].join(
    '\n',
  );
}

const EMBEDDED_ACCESS = 'Any code that opens the file: an embedded file has no keys or admin.';

function finish(spec: OperationSpec, collection: Collection, target: Target): ApiOperation {
  const asAdmin = spec.access === 'manage' && collection.protected;
  const head = 'file' in target ? setupEmbedded(target.file) : setup(target.url, asAdmin);
  return {
    id: spec.id,
    title: spec.title,
    summary: spec.summary,
    method: spec.method,
    path: spec.path,
    access: 'file' in target ? EMBEDDED_ACCESS : describeAccess(spec.access, collection.protected),
    code: `${head}\n\n${spec.call}`,
    response: spec.response,
  };
}

/** How many IDs a batch of up to BATCH_SIZE would get, given what is left. */
function batchCount(next: bigint, step: bigint, max: bigint): bigint {
  const remaining = (max - next) / step + 1n;
  return remaining < BATCH_SIZE ? remaining : BATCH_SIZE;
}

function generateOps(collection: Collection, js: string): OperationSpec[] {
  const path = `/v1/collections/${encodeURIComponent(collection.name)}/next`;
  const next = collection.next_id === null ? null : BigInt(collection.next_id);
  const step = BigInt(collection.step);
  const count = next === null ? BATCH_SIZE : batchCount(next, step, BigInt(collection.max));
  return [
    {
      id: 'next',
      title: 'Generate an ID',
      summary: 'Issue the next ID. Every call returns a new, increasing ID, never a duplicate.',
      method: 'POST',
      path,
      access: 'generate',
      call: `const id = await ids.generateId(${js});`,
      response: next === null ? exhausted(collection.name) : ok({ id: toId(next) }),
    },
    {
      id: 'next-batch',
      title: 'Generate a batch',
      summary: 'Issue several IDs in one call: all of them or none.',
      method: 'POST',
      path: `${path}?count=${count}`,
      access: 'generate',
      call: `// { first, last, count, step }\nconst batch = await ids.generateIds(${js}, ${count});`,
      response:
        next === null
          ? exhausted(collection.name)
          : ok({ first: toId(next), last: toId(next + step * (count - 1n)), count: Number(count), step: collection.step }),
    },
  ];
}

function readOps(collection: Collection, js: string): OperationSpec[] {
  const path = `/v1/collections/${encodeURIComponent(collection.name)}`;
  return [
    {
      id: 'last',
      title: 'Last ID',
      summary: 'The last issued ID and the next one, without issuing anything.',
      method: 'GET',
      path: `${path}/last`,
      access: 'read',
      call: `const { lastId, nextId } = await ids.lastId(${js});`,
      response: ok({ last_id: collection.last_id, next_id: collection.next_id }),
    },
    {
      id: 'get',
      title: 'Collection info',
      summary: 'The collection settings and its counter.',
      method: 'GET',
      path,
      access: 'read',
      call: `const collection = await ids.getCollection(${js});`,
      response: ok(collection),
    },
  ];
}

/** Skip ahead when that fits under max; an exhausted collection can only rewind to start. */
function resetOp(collection: Collection, js: string): OperationSpec {
  const start = BigInt(collection.start);
  const step = BigInt(collection.step);
  const next = collection.next_id === null ? null : BigInt(collection.next_id);
  const ahead = next === null ? null : next + step * RESET_SKIP;
  const value = next === null ? start : ahead !== null && ahead <= BigInt(collection.max) ? ahead : next;
  const isRewind = next === null;
  const call = isRewind
    ? `// Back to the start: the counter moves backwards, so allowRewind is required.\nconst result = await ids.resetCollection(${js}, { value: ${jsId(value)}, allowRewind: true });`
    : `// The next ID will be ${value}. Moving the counter backwards also needs allowRewind: true.\nconst result = await ids.resetCollection(${js}, { value: ${jsId(value)} });`;
  return {
    id: 'reset',
    title: 'Reset the counter',
    summary: 'Set the next ID. The SDK sends the confirmation (the collection name) for you.',
    method: 'POST',
    path: `/v1/collections/${encodeURIComponent(collection.name)}/reset`,
    access: 'manage',
    call,
    response: ok({
      previous_last_id: collection.last_id,
      last_id: value > start ? toId(value - step) : null,
      next_id: toId(value),
    }),
  };
}

function manageOps(collection: Collection, js: string): OperationSpec[] {
  const path = `/v1/collections/${encodeURIComponent(collection.name)}`;
  return [
    {
      id: 'update',
      title: 'Update settings',
      summary: 'Change max, protected or durability (start and step only before the first ID).',
      method: 'PATCH',
      path,
      access: 'manage',
      call: `// Send only what changes: max, protected, start, step or durability.\nconst updated = await ids.updateCollection(${js}, { max: ${jsId(collection.max)} });`,
      response: ok(collection),
    },
    resetOp(collection, js),
    {
      id: 'delete',
      title: 'Delete',
      summary: 'Remove the collection and its counter. Returns the collection as it was.',
      method: 'DELETE',
      path: `${path}?confirm=${encodeURIComponent(collection.name)}`,
      access: 'manage',
      call: `// Sends the confirmation for you.\nconst deleted = await ids.deleteCollection(${js});`,
      response: ok(collection),
    },
  ];
}

/** Where the code runs: against a server at `url`, or embedded on `file`. */
export type Target = { readonly url: string } | { readonly file: string };

/** Every operation on `collection`, for a server client or embedded code. */
export function buildApiPreview(collection: Collection, target: Target): ApiOperation[] {
  const js = jsString(collection.name);
  return [...generateOps(collection, js), ...readOps(collection, js), ...manageOps(collection, js)].map((spec) =>
    finish(spec, collection, target),
  );
}
