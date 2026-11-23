// Pure display helpers. Vue renders their results as text, never as HTML.

import { sameValue } from './fields';
import { stringifyJson } from './json';
import type { Batch, Collection, Durability, Id } from './types';

const SMALLEST_SHOWN = 0.001;
const RATIO_SCALE = 1_000_000n;
const SECONDS_PER_MINUTE = 60;
const MS_PER_SECOND = 1000;

/** An ID or `—` when there is none. */
export function formatId(id: Id | null | undefined): string {
  return id === null || id === undefined ? '—' : String(id);
}

/** Share of the collection's range not yet issued: 1 = unused, 0 = exhausted. */
export function remainingRatio({ start, max, next_id: nextId }: Pick<Collection, 'start' | 'max' | 'next_id'>): number {
  if (nextId === null || nextId === undefined) return 0;
  const total = BigInt(max) - BigInt(start) + 1n;
  const left = BigInt(max) - BigInt(nextId) + 1n;
  if (total <= 0n || left <= 0n) return 0;
  if (left >= total) return 1;
  return Number((left * RATIO_SCALE) / total) / Number(RATIO_SCALE);
}

/** 0.4567 → "45.7%", with "<0.1%" for tiny non-zero shares. */
export function formatRatio(ratio: number): string {
  if (ratio > 0 && ratio < SMALLEST_SHOWN) return '<0.1%';
  return `${(ratio * 100).toFixed(1)}%`;
}

/** `{mode: "block", size: 100}` → "block (100)". */
export function formatDurability(durability: Durability | null | undefined): string {
  if (durability?.mode === 'block') return `block (${durability.size})`;
  return durability?.mode ?? '—';
}

/** An RFC 3339 time in the viewer's locale, or the raw text if unparsable. */
export function formatTime(value: string | null | undefined, locale?: string): string {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString(locale);
}

/** Milliseconds left → "m:ss" (never negative). */
export function formatCountdown(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / MS_PER_SECOND));
  const minutes = Math.floor(seconds / SECONDS_PER_MINUTE);
  return `${minutes}:${String(seconds % SECONDS_PER_MINUTE).padStart(2, '0')}`;
}

/** The IDs a `next` call returned, as one line of text. */
export function formatBatch({ first, last, count }: Pick<Batch, 'first' | 'last' | 'count'>): string {
  return count === 1 ? `ID ${first}` : `${count} IDs: ${first} … ${last}`;
}

/** An API value for display: strings as-is, everything else as JSON. */
export function formatValue(value: unknown): string {
  if (value === null || value === undefined) return '—';
  return typeof value === 'string' ? value : (stringifyJson(value) ?? '—');
}

export type ChangeKind = 'added' | 'removed' | 'changed';
export interface ChangeRow {
  field: string;
  kind: ChangeKind;
  before: string;
  after: string;
}

/**
 * The fields an audit entry changed, field by field. With only one side
 * present (a creation or a deletion), every field is listed.
 */
export function summarizeChange(previous: unknown, next: unknown): ChangeRow[] {
  const before = asObject(previous);
  const after = asObject(next);
  const fields = [...new Set([...Object.keys(before), ...Object.keys(after)])];
  return fields
    .filter((field) => !sameValue(before[field], after[field]))
    .map((field) => ({
      field,
      kind: changeKind(field in before, field in after),
      before: field in before ? formatValue(before[field]) : '—',
      after: field in after ? formatValue(after[field]) : '—',
    }));
}

function changeKind(isBefore: boolean, isAfter: boolean): ChangeKind {
  if (!isBefore) return 'added';
  return isAfter ? 'changed' : 'removed';
}

function asObject(value: unknown): Record<string, unknown> {
  if (value === null || value === undefined) return {};
  return typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : { value };
}
