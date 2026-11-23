// Request pieces shared by the HTTP client and the embedded database, which
// both speak the API's snake_case bodies.
import type { CollectionPatch } from './types.js';

export function assertCount(count: number): void {
  if (!Number.isSafeInteger(count) || count < 1) {
    throw new RangeError('count must be an integer >= 1');
  }
}

/** Drops keys whose value is `undefined` so they are not sent. */
export function compact(fields: Readonly<Record<string, unknown>>): Readonly<Record<string, unknown>> {
  return Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined));
}

export function collectionBody(spec: CollectionPatch & { readonly name?: string }): Readonly<Record<string, unknown>> {
  return compact({
    name: spec.name,
    start: spec.start,
    step: spec.step,
    max: spec.max,
    protected: spec.protected,
    durability: spec.durability,
  });
}
