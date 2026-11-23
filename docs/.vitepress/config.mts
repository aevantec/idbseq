import { existsSync, statSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type MarkdownIt from 'markdown-it';
import { defineConfig } from 'vitepress';

const REPO = 'https://github.com/aevantec/idbseq';
const DOCS_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO_ROOT = resolve(DOCS_DIR, '..');
// GitHub Pages project site; set DOCS_BASE=/ for a custom domain.
const BASE = process.env.DOCS_BASE ?? '/idbseq/';

// Maintainer-only files that stay in the repository but are not site pages
// (links to them go to GitHub).
const EXCLUDED = ['PLAN.md', 'adr/template.md', 'README.md', 'benchmarks.md', 'node_modules/**'];

/** Is `target` (an absolute path) a page of this site? */
function isSitePage(target: string): boolean {
  const rel = relative(DOCS_DIR, target);
  if (rel.startsWith('..')) {
    return false;
  }
  return !EXCLUDED.includes(rel.replaceAll('\\', '/'));
}

/**
 * The Markdown in docs/ is also read on GitHub, so it links to files beside it
 * (`../sdk/node`, `../api/openapi.json`, `template.md`). Those are not pages
 * here: point them at the repository instead.
 */
function repoLinks(md: MarkdownIt): void {
  md.core.ruler.push('idbseq-repo-links', (state) => {
    const from = (state.env as { path?: string }).path;
    if (from === undefined) {
      return;
    }
    for (const block of state.tokens) {
      for (const token of block.children ?? []) {
        const href = token.type === 'link_open' ? token.attrGet('href') : null;
        if (href !== null && href.startsWith('/') && existsSync(resolve(DOCS_DIR, 'public', `.${href}`))) {
          // Static files (the API explorer) are not routed, so they need the base here.
          token.attrSet('href', `${BASE}${href.slice(1)}`);
          continue;
        }
        if (href === null || /^([a-z]+:|#|\/)/i.test(href)) {
          continue;
        }
        const [path, hash = ''] = href.split('#');
        const target = resolve(dirname(from), decodeURIComponent(path));
        if (isSitePage(target)) {
          // Included READMEs link relative to their own folder; re-anchor on this page.
          const fromPage = relative(dirname(from), target).replaceAll('\\', '/');
          token.attrSet('href', `${fromPage}${hash ? `#${hash}` : ''}`);
          continue;
        }
        const kind = existsSync(target) && statSync(target).isDirectory() ? 'tree' : 'blob';
        const repoPath = relative(REPO_ROOT, target).replaceAll('\\', '/');
        token.attrSet('href', `${REPO}/${kind}/main/${repoPath}${hash ? `#${hash}` : ''}`);
      }
    }
  });
}

export default defineConfig({
  title: 'idbseq',
  description: 'A durable ID sequence server: unique, increasing 64-bit IDs over HTTP/JSON.',
  lang: 'en-US',
  base: BASE,
  cleanUrls: true,
  lastUpdated: true,
  srcExclude: EXCLUDED,
  rewrites: { 'adr/README.md': 'adr/index.md' },
  ignoreDeadLinks: 'localhostLinks',
  head: [
    ['meta', { name: 'theme-color', content: '#c2410c' }],
    ['link', { rel: 'icon', type: 'image/svg+xml', href: `${BASE}logo.svg` }],
  ],
  markdown: {
    config: repoLinks,
  },
  themeConfig: {
    // "id" and "+": an incrementing ID. Inverted for the dark theme.
    logo: { light: '/logo.svg', dark: '/logo-dark.svg', alt: 'idbseq' },
    nav: [
      { text: 'Guide', link: '/guide/introduction', activeMatch: '/guide/' },
      { text: 'Clients', link: '/clients/cli', activeMatch: '/clients' },
      { text: 'HTTP API', link: '/API' },
      { text: 'API explorer', link: '/api-explorer.html', target: '_self' },
      { text: 'Server', link: '/guide/server', activeMatch: '/(guide/(server|deployment|monitoring|console|security)|operations)' },
    ],
    sidebar: [
      {
        text: 'Introduction',
        items: [
          { text: 'What is idbseq?', link: '/guide/introduction' },
          { text: 'Getting started', link: '/guide/getting-started' },
          { text: 'Embedding in depth', link: '/guide/embedded' },
        ],
      },
      {
        text: 'Concepts',
        items: [
          { text: 'Collections', link: '/guide/collections' },
          { text: 'IDs, order and gaps', link: '/guide/ids' },
          { text: 'Durability and speed', link: '/guide/durability' },
          { text: 'Retries and idempotency', link: '/guide/retries' },
        ],
      },
      {
        text: 'Server',
        items: [
          { text: 'Running a server', link: '/guide/server' },
          { text: 'Admin, API keys and scopes', link: '/guide/security' },
          { text: 'Configuration and commands', link: '/operations' },
          { text: 'Deployment', link: '/guide/deployment' },
          { text: 'Health checks and monitoring', link: '/guide/monitoring' },
          { text: 'Admin console', link: '/guide/console' },
        ],
      },
      {
        text: 'Server clients',
        items: [
          { text: 'Command line (idbseq)', link: '/clients/cli' },
          { text: 'Node.js SDK', link: '/clients/node' },
          { text: 'AI agents (MCP server)', link: '/clients/mcp' },
          { text: 'curl, Python, Go, Java', link: '/clients' },
        ],
      },
      {
        text: 'Reference',
        items: [
          { text: 'HTTP API', link: '/API' },
          { text: 'API explorer (OpenAPI)', link: '/api-explorer.html', target: '_self' },
          { text: 'Architecture decisions', link: '/adr/' },
          { text: 'Releasing', link: '/releasing' },
          { text: 'FAQ', link: '/guide/faq' },
        ],
      },
    ],
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: REPO }],
    editLink: {
      // Runs in the browser, so it cannot use anything outside its own body.
      pattern: ({ filePath }) => {
        const included: Record<string, string> = {
          'clients/cli.md': 'crates/idbseq-cli/README.md',
          'clients/node.md': 'sdk/node/README.md',
          'clients/mcp.md': 'sdk/mcp/README.md',
          'adr/index.md': 'docs/adr/README.md',
        };
        const path = included[filePath] ?? `docs/${filePath}`;
        return `https://github.com/aevantec/idbseq/edit/main/${path}`;
      },
      text: 'Edit this page on GitHub',
    },
    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright © 2026 Aevantec',
    },
    outline: { level: [2, 3] },
  },
});
