// End-to-end tests for the admin console against a running idbseqd with
// `ui = true`. scripts/e2e.sh starts one and sets IDBSEQ_E2E_URL and
// IDBSEQ_E2E_ADMIN_PASSWORD: run `scripts/e2e.sh ui`.
import { defineConfig, devices } from '@playwright/test';

const url = process.env.IDBSEQ_E2E_URL;
if (!url) throw new Error('Set IDBSEQ_E2E_URL (or run scripts/e2e.sh ui).');

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: process.env.CI ? [['list'], ['github']] : 'list',
  use: {
    baseURL: `${url}/ui/`,
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
