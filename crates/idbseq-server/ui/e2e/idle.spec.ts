import type { Page } from '@playwright/test';

import { expect, signIn, test } from './helpers';

const MINUTE = 60_000;
const TTL = 15 * MINUTE;

/**
 * The browser's clock is fake, so answer refreshes relative to it, as the
 * server does relative to its own. Returns how many refreshes were sent.
 */
async function answerRefreshes(page: Page): Promise<() => number> {
  let count = 0;
  await page.route('**/v1/auth/refresh', async (route) => {
    count += 1;
    const now = await page.evaluate(() => Date.now());
    const body = { success: true, data: { expires_at: new Date(now + TTL).toISOString() }, error: null };
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
  });
  return () => count;
}

test.beforeEach(async ({ page }) => {
  await page.clock.install();
  await signIn(page);
});

test('asks a minute before an idle sign-out, and Stay signed in extends the session', async ({ page }) => {
  const refreshes = await answerRefreshes(page);
  const prompt = page.getByRole('alertdialog', { name: 'Are you still there?' });

  await page.clock.runFor(TTL - 2 * MINUTE);
  await expect(prompt).toBeHidden();
  await page.clock.runFor(MINUTE + 1000);
  await expect(prompt).toBeVisible();
  await expect(prompt).toContainText('signed out in 0:');

  await prompt.getByRole('button', { name: 'Stay signed in' }).click();

  await expect(prompt).toBeHidden();
  expect(refreshes()).toBe(1);
  await expect(page.locator('.countdown')).toContainText(/1[45]:\d\d/);
  await page.clock.runFor(5 * MINUTE);
  await expect(page.getByRole('heading', { name: 'Collections', level: 2 })).toBeVisible();
});

test('without an answer it signs out and says why', async ({ page }) => {
  await page.clock.runFor(TTL - MINUTE + 1000);
  await expect(page.getByRole('alertdialog', { name: 'Are you still there?' })).toBeVisible();

  await page.clock.runFor(MINUTE);

  await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
  await expect(page.getByText('You were signed out after a period of inactivity.')).toBeVisible();
});

test('activity keeps the session alive without asking', async ({ page }) => {
  const refreshes = await answerRefreshes(page);

  for (let minute = 0; minute < 20; minute += 1) {
    await page.mouse.move(100 + minute, 200);
    await page.clock.runFor(MINUTE);
  }

  await expect(page.getByRole('alertdialog')).toBeHidden();
  await expect(page.getByRole('heading', { name: 'Collections', level: 2 })).toBeVisible();
  expect(refreshes()).toBeGreaterThanOrEqual(15);
});

test('Sign out in the prompt signs out', async ({ page }) => {
  await page.clock.runFor(TTL - MINUTE + 1000);
  const prompt = page.getByRole('alertdialog', { name: 'Are you still there?' });

  await prompt.getByRole('button', { name: 'Sign out' }).click();

  await expect(page.getByText('Signed out.')).toBeVisible();
});
