import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, test } from 'vitest';

import { LOGO_GLYPHS, LOGO_RADIUS, LOGO_SIZE } from '@/lib/brand';

const REPO = resolve(__dirname, '../../../../..');
const read = (path: string) => readFileSync(resolve(REPO, path), 'utf8');

describe('the logo', () => {
  test('the console favicon is the docs logo', () => {
    expect(read('crates/idbseq-server/ui/public/favicon.svg')).toBe(read('docs/public/logo.svg'));
  });

  test('the console mark draws the same glyphs and square as the logo files', () => {
    for (const file of ['docs/public/logo.svg', 'docs/public/logo-dark.svg']) {
      const svg = read(file);
      expect(svg, file).toContain(`viewBox="0 0 ${LOGO_SIZE} ${LOGO_SIZE}"`);
      expect(svg, file).toContain(`rx="${LOGO_RADIUS}"`);
      expect(svg, file).toContain(`d="${LOGO_GLYPHS}"`);
    }
  });

  test('the light logo is white on black and the dark one black on white', () => {
    expect(read('docs/public/logo.svg')).toMatch(/fill="#000"[\s\S]*fill="#fff"/);
    expect(read('docs/public/logo-dark.svg')).toMatch(/fill="#fff"[\s\S]*fill="#000"/);
  });
});
