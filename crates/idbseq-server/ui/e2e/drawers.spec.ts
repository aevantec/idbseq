import { createCollection, expect, rail, signIn, test, unique } from './helpers';

test.beforeEach(async ({ page }) => signIn(page));

test('a New collection error is gone after closing and reopening', async ({ page }) => {
  const name = unique('ui-drawer');
  await createCollection(page, name);
  await rail(page).getByRole('link', { name: 'Collections' }).click();
  await page.getByRole('button', { name: 'New collection' }).click();
  const panel = page.getByRole('dialog', { name: 'New collection' });
  await panel.getByLabel('Name', { exact: true }).fill(name);
  await panel.getByRole('button', { name: 'Create collection' }).click();
  await expect(panel.getByRole('alert')).toContainText('COLLECTION_EXISTS');

  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'New collection' }).click();

  await expect(page.getByRole('dialog', { name: 'New collection' })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'New collection' }).getByRole('alert')).toHaveCount(0);
});

test('a Settings error is gone after closing and reopening', async ({ page }) => {
  const name = unique('ui-drawer-settings');
  await createCollection(page, name, { Start: '10' });
  await page.getByRole('button', { name: 'Settings' }).click();
  const panel = page.getByRole('dialog', { name: `${name} settings` });
  await panel.getByLabel('Max', { exact: true }).fill('5');
  await panel.getByRole('button', { name: 'Save changes' }).click();
  await expect(panel.getByRole('alert')).toContainText('VALIDATION_FAILED');

  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Settings' }).click();

  await expect(panel).toBeVisible();
  await expect(panel.getByRole('alert')).toHaveCount(0);
});

test('a New API key error is gone after closing and reopening', async ({ page }) => {
  await page.route('**/v1/keys', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    const error = { code: 'VALIDATION_FAILED', message: 'name: too long', details: { fields: { name: 'too long' } } };
    return route.fulfill({ status: 400, contentType: 'application/json', body: JSON.stringify({ success: false, data: null, error }) });
  });
  await rail(page).getByRole('link', { name: 'API keys' }).click();
  await page.getByRole('button', { name: 'New API key' }).click();
  const panel = page.getByRole('dialog', { name: 'New API key' });
  await panel.getByLabel('Name', { exact: true }).fill('drawer test');
  await panel.getByRole('checkbox', { name: /read/ }).check();
  await panel.getByRole('button', { name: 'Create key' }).click();
  await expect(panel.getByRole('alert')).toContainText('VALIDATION_FAILED');

  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'New API key' }).click();

  await expect(panel).toBeVisible();
  await expect(panel.getByRole('alert')).toHaveCount(0);
});
