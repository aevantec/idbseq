import { defineConfig } from 'vitest/config';

export default defineConfig({
  // Also transform src/native.cts (Vite's default skips .cts).
  esbuild: { include: /\.[cm]?tsx?$/ },
  test: {
    include: ['test/**/*.test.ts'],
    exclude: ['test/e2e/**', 'node_modules/**'],
    coverage: {
      provider: 'v8',
      include: ['src/**/*.ts', 'src/**/*.cts'],
      reporter: ['text', 'lcov'],
      thresholds: { lines: 80, functions: 80, branches: 80, statements: 80 },
    },
  },
});
