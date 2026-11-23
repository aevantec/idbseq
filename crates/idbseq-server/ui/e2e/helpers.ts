import { expect, test as base, type Page } from '@playwright/test';

export const PASSWORD = process.env.IDBSEQ_E2E_ADMIN_PASSWORD ?? '';

/** A name unique to this run, so tests never see each other's data. */
export function unique(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/** Fails the test on any console error, including Content-Security-Policy violations. */
export const test = base.extend({
  page: async ({ page }, use) => {
    const problems: string[] = [];
    page.on('console', (entry) => {
      if (entry.type() === 'error') problems.push(entry.text());
    });
    page.on('pageerror', (error) => problems.push(error.message));
    await use(page);
    // Failed requests the test provokes on purpose (401, 400) are logged by the browser.
    expect(problems.filter((text) => !/status of 4\d\d/.test(text)), 'browser console errors').toEqual([]);
  },
});

export { expect };

export function rail(page: Page) {
  return page.getByRole('navigation', { name: 'Console' });
}

export async function signIn(page: Page): Promise<void> {
  await page.goto('./');
  await page.getByLabel('Password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page.getByRole('heading', { name: 'Collections', level: 2 })).toBeVisible();
}

/** Create a collection through the "New collection" panel and open it. */
export async function createCollection(page: Page, name: string, fields: Record<string, string> = {}): Promise<void> {
  await rail(page).getByRole('link', { name: 'Collections' }).click();
  await page.getByRole('button', { name: 'New collection' }).click();
  const panel = page.getByRole('dialog', { name: 'New collection' });
  await panel.getByLabel('Name', { exact: true }).fill(name);
  for (const [label, value] of Object.entries(fields)) {
    await panel.getByLabel(label, { exact: true }).fill(value);
  }
  await panel.getByRole('button', { name: 'Create collection' }).click();
  await expect(page.getByRole('heading', { name, level: 1 })).toBeVisible();
}

/** The value shown on a stat card ("Last ID", "Next ID", …). */
export function stat(page: Page, label: string) {
  return page.locator('dl.summary > div').filter({ has: page.locator('dt', { hasText: label }) }).locator('dd');
}

export async function takeIds(page: Page, count: string): Promise<void> {
  await page.getByLabel('Count', { exact: true }).fill(count);
  await page.getByRole('button', { name: 'Take IDs' }).click();
}
