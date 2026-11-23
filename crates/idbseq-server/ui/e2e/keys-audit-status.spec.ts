import { createCollection, expect, rail, signIn, stat, test, unique } from './helpers';
import type { Page } from '@playwright/test';

test.beforeEach(async ({ page }) => signIn(page));

async function openNewKey(page: Page) {
  await rail(page).getByRole('link', { name: 'API keys' }).click();
  await page.getByRole('button', { name: 'New API key' }).click();
  return page.getByRole('dialog', { name: 'New API key' });
}

test('creates a key, shows the secret once, and the key works', async ({ page, request }) => {
  const collection = unique('ui-keyed');
  const keyName = unique('ui-billing');
  await createCollection(page, collection);

  const panel = await openNewKey(page);
  await panel.getByLabel('Name', { exact: true }).fill(keyName);
  await panel.getByRole('checkbox', { name: /generate/ }).check();
  await panel.getByLabel('Collections', { exact: true }).fill(collection);
  await panel.getByRole('button', { name: 'Create key' }).click();

  const secret = await page.getByLabel('The new API key').inputValue();
  expect(secret).toMatch(/^idb_[a-z0-9]+_/);
  const taken = await request.post(`/v1/collections/${collection}/next`, { headers: { authorization: `Bearer ${secret}` } });
  expect(taken.status()).toBe(200);

  await page.getByRole('button', { name: 'I have stored it' }).click();
  await expect(page.getByLabel('The new API key')).toHaveCount(0);
  const row = page.getByRole('row', { name: new RegExp(keyName) });
  await expect(row).toContainText('generate');
  await expect(row).toContainText(collection);
});

test('requires a scope', async ({ page }) => {
  const panel = await openNewKey(page);
  await panel.getByLabel('Name', { exact: true }).fill('no-scope');

  await panel.getByRole('button', { name: 'Create key' }).click();

  await expect(panel.getByRole('alert')).toHaveText('Choose at least one scope.');
});

test('shows names as text, never as HTML', async ({ page }) => {
  const hostile = `<img src=x data-pwned=1>${unique('x')}`;
  const panel = await openNewKey(page);
  await panel.getByLabel('Name', { exact: true }).fill(hostile);
  await panel.getByRole('checkbox', { name: /read/ }).check();
  await panel.getByRole('button', { name: 'Create key' }).click();
  await page.getByRole('button', { name: 'I have stored it' }).click();

  await expect(page.getByRole('cell', { name: hostile })).toBeVisible();
  await expect(page.locator('img[data-pwned]')).toHaveCount(0);
});

test('revokes a key after confirmation', async ({ page }) => {
  const name = unique('ui-revoke');
  const panel = await openNewKey(page);
  await panel.getByLabel('Name', { exact: true }).fill(name);
  await panel.getByRole('checkbox', { name: /read/ }).check();
  await panel.getByRole('button', { name: 'Create key' }).click();
  await page.getByRole('button', { name: 'I have stored it' }).click();

  await page.getByRole('row', { name: new RegExp(name) }).getByRole('button', { name: 'Revoke', exact: true }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'Revoke key' }).click();

  await expect(page.getByText(new RegExp(`Revoked ${name}`))).toBeVisible();
  await expect(page.getByRole('row', { name: new RegExp(name) })).toHaveCount(0);
});

test('the audit log shows changes, filters, and opens an entry', async ({ page }) => {
  const name = unique('ui-audited');
  await createCollection(page, name);
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByRole('dialog').getByLabel('Step', { exact: true }).fill('3');
  await page.getByRole('dialog').getByRole('button', { name: 'Save changes' }).click();
  await expect(page.getByText(`Saved ${name}.`)).toBeVisible();

  await rail(page).getByRole('link', { name: 'Audit log' }).click();
  await page.getByRole('searchbox', { name: 'Collection' }).fill(name);
  await page.getByLabel('Action').selectOption('collection.update');

  const row = page.getByRole('row', { name: /collection\.update/ });
  await expect(row).toHaveCount(1);
  await expect(row).toContainText('step: 1 → 3');
  await expect(row).toContainText('admin');

  await row.getByRole('button').click();
  const details = page.getByRole('dialog', { name: 'collection.update' });
  await expect(details.getByRole('row', { name: /step/ })).toContainText('1');
  await expect(details.getByRole('row', { name: /step/ })).toContainText('3');
});

test('a collection links to its audit entries', async ({ page }) => {
  const name = unique('ui-linked');
  await createCollection(page, name);

  await page.getByRole('link', { name: 'Open in audit log →' }).click();

  await expect(page.getByRole('searchbox', { name: 'Collection' })).toHaveValue(name);
  await expect(page.getByRole('row', { name: /collection\.create/ })).toHaveCount(1);
});

test('the status page reports health and readiness', async ({ page }) => {
  await rail(page).getByRole('link', { name: 'Status' }).click();

  await expect(stat(page, 'Health (/healthz)')).toHaveText('ok');
  await expect(stat(page, 'Readiness (/readyz)')).toHaveText('ready');
  await expect(stat(page, 'API version')).toHaveText(/^\d+\.\d+\.\d+/);
});

test('every page has the same icon-only Refresh button', async ({ page }) => {
  const name = unique('ui-refresh');
  await createCollection(page, name);
  const pages = ['API keys', 'Audit log', 'Status'];
  const check = async (): Promise<void> => {
    const refresh = page.getByRole('button', { name: 'Refresh', exact: true });
    await expect(refresh).toBeVisible();
    await expect(refresh).toHaveText('');
    await expect(refresh).toHaveAttribute('title', 'Refresh');
  };
  await check();
  for (const link of pages) {
    await rail(page).getByRole('link', { name: link }).click();
    await check();
  }
});
