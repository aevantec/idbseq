// Lossless JSON for 64-bit IDs. JavaScript numbers are exact only up to 2^53,
// so integers outside that range are parsed as BigInt and written back as
// bare JSON integers, never rounded.

const BIG = '\u0000big:';
const NUMBER = /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/y;
const INTEGER = /^-?\d+$/;
const BIG_PLACEHOLDER = /"\\u0000big:(-?\d+)"/g;

/** JSON.parse, but integers beyond Number.MAX_SAFE_INTEGER become BigInt. */
export function parseJson(text: string): unknown {
  return JSON.parse(quoteUnsafeIntegers(text), (_key, value: unknown) => {
    if (typeof value !== 'string' || !value.startsWith(BIG)) return value;
    const digits = value.slice(BIG.length);
    return INTEGER.test(digits) ? BigInt(digits) : value;
  });
}

/** JSON.stringify, writing BigInt values as plain JSON integers. `indent` pretty-prints. */
export function stringifyJson(value: unknown, indent?: number): string | undefined {
  const text = JSON.stringify(
    value,
    (_key, item: unknown) => (typeof item === 'bigint' ? `${BIG}${item}` : item),
    indent,
  ) as string | undefined;
  return text === undefined ? text : text.replace(BIG_PLACEHOLDER, '$1');
}

function quoteUnsafeIntegers(text: string): string {
  const parts: string[] = [];
  let copied = 0;
  let index = 0;
  while (index < text.length) {
    const char = text.charAt(index);
    if (char === '"') {
      index = stringEnd(text, index);
      continue;
    }
    if (char !== '-' && (char < '0' || char > '9')) {
      index += 1;
      continue;
    }
    NUMBER.lastIndex = index;
    const token = NUMBER.exec(text)?.[0] ?? char;
    if (isUnsafeInteger(token)) {
      parts.push(text.slice(copied, index), JSON.stringify(`${BIG}${token}`));
      copied = index + token.length;
    }
    index += token.length;
  }
  parts.push(text.slice(copied));
  return parts.join('');
}

function stringEnd(text: string, start: number): number {
  let index = start + 1;
  while (index < text.length) {
    const char = text.charAt(index);
    if (char === '\\') index += 2;
    else if (char === '"') return index + 1;
    else index += 1;
  }
  return text.length;
}

function isUnsafeInteger(token: string): boolean {
  return INTEGER.test(token) && !Number.isSafeInteger(Number(token));
}
