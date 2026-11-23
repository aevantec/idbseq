import { fileURLToPath, URL } from 'node:url';

import tailwindcss from '@tailwindcss/vite';
import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vitest/config';

// `npm run dev` proxies the API to a local idbseqd (IDBSEQ_URL, default
// 127.0.0.1:7878). `npm run build` writes dist/, which idbseqd embeds.
const api = process.env.IDBSEQ_URL ?? 'http://127.0.0.1:7878';

export default defineConfig({
  base: '/ui/',
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    assetsInlineLimit: 0, // no data: URLs; every asset is a file under the CSP
    modulePreload: { polyfill: false }, // the polyfill is an inline script
    sourcemap: false,
  },
  server: {
    proxy: {
      '/v1': api,
      '/healthz': api,
      '/readyz': api,
    },
  },
  test: {
    environment: 'jsdom',
    include: ['tests/unit/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      include: ['src/lib/**/*.ts', 'src/session.ts'],
      exclude: ['src/lib/utils.ts'],
      thresholds: { lines: 80, branches: 80, functions: 80, statements: 80 },
    },
  },
});
