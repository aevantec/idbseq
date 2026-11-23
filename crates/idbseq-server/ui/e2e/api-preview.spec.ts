import { createCollection, expect, signIn, takeIds, test, unique } from './helpers';

test.beforeEach(async ({ page }) => signIn(page));

test('shows Node.js code and the real response for each operation', async ({ page, baseURL }) => {
  const name = unique('ui-preview');
  await createCollection(page, name, { Start: '500' });
  await takeIds(page, '2');

  await page.getByRole('button', { name: 'API preview' }).click();
  const panel = page.getByRole('dialog', { name: 'API preview' });
  const code = panel.getByLabel('Node.js code', { exact: true });

  await expect(code).toContainText(`const id = await ids.generateId('${name}');`);
  await expect(code).toContainText(`url: '${new URL(baseURL ?? '').origin}'`);
  await expect(panel.getByText(`/v1/collections/${name}/next`, { exact: true })).toBeVisible();
  await expect(panel.getByLabel('response', { exact: true })).toContainText('"id": 502');

  await expect(panel.getByRole('link', { name: 'Node.js SDK docs' })).toHaveAttribute('href', /\/clients\/node$/);

  await panel.getByRole('button', { name: /Last ID/ }).click();
  await expect(panel.getByLabel('response', { exact: true })).toContainText('"last_id": 501');
  await expect(panel.getByText('An API key with the read scope')).toBeVisible();
});

test('copies the code', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  const name = unique('ui-preview-copy');
  await createCollection(page, name);

  await page.getByRole('button', { name: 'API preview' }).click();
  const panel = page.getByRole('dialog', { name: 'API preview' });
  await panel.getByRole('button', { name: 'Copy Node.js code' }).click();

  await expect(panel.getByRole('button', { name: 'Node.js code copied' })).toBeVisible();
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toContain(`await ids.generateId('${name}');`);
  expect(copied.startsWith("import { IdbSeqClient } from 'idbseq';")).toBe(true);
});
