import { describe, expect, test } from 'vitest';

import { tokenize, type Token } from '@/lib/highlight';

const kinds = (tokens: Token[]) => tokens.filter((t) => t.kind !== 'plain').map((t) => [t.kind, t.text]);

describe('tokenize', () => {
  test('marks keywords, strings, numbers and comments in JavaScript', () => {
    const tokens = tokenize("const id = await ids.generateId('orders', 10n); // done", 'node');

    expect(kinds(tokens)).toEqual([
      ['keyword', 'const'],
      ['keyword', 'await'],
      ['string', "'orders'"],
      ['number', '10n'],
      ['comment', '// done'],
    ]);
  });

  test('keeps escaped quotes inside strings', () => {
    const tokens = tokenize("const id = await ids.generateId('it\\'s');", 'node');

    expect(kinds(tokens)).toEqual([
      ['keyword', 'const'],
      ['keyword', 'await'],
      ['string', "'it\\'s'"],
    ]);
  });

  test('marks JSON keys, values and literals', () => {
    const tokens = tokenize('{ "id": 9223372036854775807, "error": null, "ok": true }', 'json');

    expect(kinds(tokens)).toEqual([
      ['property', '"id"'],
      ['number', '9223372036854775807'],
      ['property', '"error"'],
      ['keyword', 'null'],
      ['property', '"ok"'],
      ['keyword', 'true'],
    ]);
  });

  test('loses nothing: the tokens join back into the input', () => {
    const code = "const a = 'x';\n  // y\n$b = \"z\"; { \"k\": [1, 2] } @#";
    for (const language of ['node', 'json'] as const) {
      expect(tokenize(code, language).map((t) => t.text).join('')).toBe(code);
    }
  });

  test('an unterminated string runs to the end of the line', () => {
    expect(tokenize("x = 'open\ny", 'node').map((t) => t.text).join('')).toBe("x = 'open\ny");
  });
});
