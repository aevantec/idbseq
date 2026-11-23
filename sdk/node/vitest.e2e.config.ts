import { defineConfig } from 'vitest/config';

export default defineConfig({
  // Transform src/native.cts too (Vite leaves .cts files alone by default).
  esbuild: { include: /\.[cm]?tsx?$/ },
  test: {
    include: ['test/e2e/**/*.test.ts'],
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
});
