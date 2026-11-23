// Turn the collection form's text fields into API settings, with the server's
// own bounds: start and max are i64, step a u64, a block size a u32.

import { BLOCK_SIZE_MAX, STEP_MAX, parseInteger, type IntegerBounds } from './fields';
import type { CollectionSettings, Durability } from './types';

export interface CollectionFormValues {
  name: string;
  start: string;
  step: string;
  max: string;
  mode: 'strict' | 'block';
  blockSize: string;
  isProtected: boolean;
}

const NUMERIC: ReadonlyArray<['start' | 'step' | 'max', IntegerBounds]> = [
  ['start', { label: 'Start' }],
  ['step', { label: 'Step', min: 1, max: STEP_MAX }],
  ['max', { label: 'Max' }],
];

/** Validated settings, or the problems to show. Empty optional fields are left out on create. */
export function readCollectionForm(
  values: CollectionFormValues,
  { isCreate }: { isCreate: boolean },
): { value: CollectionSettings; errors: string[] } {
  const errors: string[] = [];
  const settings: CollectionSettings = {};
  if (isCreate) {
    const name = values.name.trim();
    if (name) settings.name = name;
    else errors.push('Name is required.');
  }
  for (const [field, bounds] of NUMERIC) {
    const text = values[field].trim();
    if (text === '' && isCreate) continue;
    const parsed = parseInteger(text, bounds);
    if (parsed.error !== undefined) errors.push(parsed.error);
    else settings[field] = parsed.value;
  }
  settings.durability = readDurability(values, errors);
  settings.protected = values.isProtected;
  return { value: settings, errors };
}

function readDurability(values: CollectionFormValues, errors: string[]): Durability {
  if (values.mode !== 'block') return { mode: 'strict' };
  const parsed = parseInteger(values.blockSize, { label: 'Block size', min: 1, max: BLOCK_SIZE_MAX });
  if (parsed.error !== undefined) {
    errors.push(parsed.error);
    return { mode: 'block', size: 0 };
  }
  return { mode: 'block', size: Number(parsed.value) };
}
