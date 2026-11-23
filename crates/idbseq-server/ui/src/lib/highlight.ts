// A small syntax highlighter for the API preview's code samples. It returns
// tokens that the template renders as text inside <span>s: no HTML strings
// and no inline styles, so it fits the console's Content-Security-Policy.

export type Language = 'node' | 'json';
export type TokenKind = 'plain' | 'keyword' | 'string' | 'number' | 'comment' | 'property';

export interface Token {
  kind: TokenKind;
  text: string;
}

const LITERALS = ['true', 'false', 'null'];
const KEYWORDS: Record<Language, ReadonlySet<string>> = {
  node: new Set([
    ...LITERALS, 'async', 'await', 'catch', 'const', 'else', 'export', 'from', 'function',
    'if', 'import', 'let', 'new', 'return', 'throw', 'try', 'undefined',
  ]),
  json: new Set(LITERALS),
};

// An unterminated string runs to the end of its line.
const PATTERN =
  /(?<comment>\/\/[^\n]*)|(?<string>'(?:\\.|[^'\\\n])*'?|"(?:\\.|[^"\\\n])*"?)|(?<number>\d[\d_]*n?)|(?<word>[A-Za-z_]\w*)/y;
const BEFORE_COLON = /^\s*:/;

/** Split `code` into tokens. Joining their text gives `code` back exactly. */
export function tokenize(code: string, language: Language): Token[] {
  const tokens: Token[] = [];
  let index = 0;
  while (index < code.length) {
    PATTERN.lastIndex = index;
    const match = PATTERN.exec(code);
    const text = match?.[0] ?? code.charAt(index);
    const kind = match?.groups ? classify(match.groups, text, language, code.slice(index + text.length)) : 'plain';
    push(tokens, { kind, text });
    index += text.length;
  }
  return tokens;
}

function classify(groups: Record<string, string | undefined>, text: string, language: Language, rest: string): TokenKind {
  if (groups.comment !== undefined) return 'comment';
  if (groups.string !== undefined) {
    return language === 'json' && BEFORE_COLON.test(rest) ? 'property' : 'string';
  }
  if (groups.number !== undefined) return 'number';
  return KEYWORDS[language].has(text) ? 'keyword' : 'plain';
}

/** Append, merging neighbouring plain text into one token. */
function push(tokens: Token[], token: Token): void {
  const last = tokens.at(-1);
  if (last && last.kind === 'plain' && token.kind === 'plain') {
    tokens[tokens.length - 1] = { kind: 'plain', text: last.text + token.text };
  } else {
    tokens.push(token);
  }
}
