import { createCollection, expect, signIn, stat, takeIds, test, unique } from './helpers';

test.beforeEach(async ({ page }) => signIn(page));

test('creates a collection, takes IDs, and lists it in the sidebar', async ({ page }) => {
  const name = unique('ui-orders');
  await createCollection(page, name, { Start: '1000' });

  await takeIds(page, '3');

  await expect(page.getByText('Took 3 IDs: 1000 … 1002.')).toBeVisible();
  await expect(stat(page, 'Last ID')).toHaveText('1002');
  await expect(stat(page, 'Next ID')).toHaveText('1003');
  await expect(stat(page, 'Range left')).toHaveText('100.0%');

  await page.getByLabel('Filter by name prefix').fill(name);
  const list = page.getByRole('navigation', { name: 'Collections', exact: true });
  await expect(list.getByRole('link')).toHaveCount(1);
  await expect(list.getByRole('link', { name })).toHaveAttribute('aria-current', 'page');
});

test('shows 64-bit IDs exactly', async ({ page }) => {
  const name = unique('ui-big');
  await createCollection(page, name, { Start: '9223372036854775800' });

  await takeIds(page, '2');

  await expect(page.getByText('Took 2 IDs: 9223372036854775800 … 9223372036854775801.')).toBeVisible();
  await expect(stat(page, 'Next ID')).toHaveText('9223372036854775802');
});

test('edits settings and sends only what changed', async ({ page }) => {
  const name = unique('ui-edit');
  await createCollection(page, name);

  await page.getByRole('button', { name: 'Settings' }).click();
  const panel = page.getByRole('dialog', { name: `${name} settings` });
  const patch = page.waitForRequest((request) => request.method() === 'PATCH');
  await panel.getByLabel('Step', { exact: true }).fill('5');
  await panel.getByLabel('Durability').selectOption('block');
  await panel.getByLabel('Block size').fill('50');
  await panel.getByRole('button', { name: 'Save changes' }).click();

  expect(JSON.parse((await patch).postData() ?? '')).toEqual({ step: 5, durability: { mode: 'block', size: 50 } });
  await expect(stat(page, 'Durability')).toHaveText('block (50)');
});

test('shows validation errors from the server', async ({ page }) => {
  const name = unique('ui-invalid');
  await createCollection(page, name, { Start: '10' });

  await page.getByRole('button', { name: 'Settings' }).click();
  const panel = page.getByRole('dialog', { name: `${name} settings` });
  await panel.getByLabel('Max', { exact: true }).fill('5');
  await panel.getByRole('button', { name: 'Save changes' }).click();

  await expect(panel.getByRole('alert')).toContainText('VALIDATION_FAILED');
});

test('reset needs the name typed and refuses to rewind without permission', async ({ page }) => {
  const name = unique('ui-reset');
  await createCollection(page, name);
  await takeIds(page, '10');
  await expect(stat(page, 'Next ID')).toHaveText('11');

  await page.getByRole('button', { name: 'Settings' }).click();
  const panel = page.getByRole('dialog', { name: `${name} settings` });
  await panel.getByRole('button', { name: 'Reset counter', exact: true }).click();
  const confirm = page.getByRole('alertdialog');
  await expect(confirm.getByRole('button', { name: 'Reset collection' })).toBeDisabled();
  await confirm.getByLabel(`Type ${name} to confirm`).fill(name);
  await confirm.getByLabel(`Type ${name} to confirm`).press('Enter');
  await expect(panel.getByRole('alert')).toContainText('allow_rewind');

  await panel.getByRole('checkbox', { name: 'Allow rewind' }).check();
  await panel.getByRole('button', { name: 'Reset counter', exact: true }).click();
  await confirm.getByLabel(`Type ${name} to confirm`).fill(name);
  await confirm.getByRole('button', { name: 'Reset collection' }).click();

  await expect(page.getByText(`Reset ${name}: the next ID is 1.`)).toBeVisible();
  await expect(stat(page, 'Next ID')).toHaveText('1');
});

test('delete needs the name typed, and cancelling keeps the collection', async ({ page }) => {
  const name = unique('ui-delete');
  await createCollection(page, name);
  await page.getByRole('button', { name: 'Settings' }).click();
  const panel = page.getByRole('dialog', { name: `${name} settings` });

  await panel.getByRole('button', { name: 'Delete collection', exact: true }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
  await expect(panel.getByRole('button', { name: 'Delete collection', exact: true })).toBeVisible();

  await panel.getByRole('button', { name: 'Delete collection', exact: true }).click();
  await page.getByRole('alertdialog').getByLabel(`Type ${name} to confirm`).fill(name);
  await page.getByRole('alertdialog').getByRole('button', { name: 'Delete collection' }).click();

  await expect(page.getByText(`Deleted ${name}.`)).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Collections', level: 1 })).toBeVisible();
});

test('the collection page lists its recent changes', async ({ page }) => {
  const name = unique('ui-activity');
  await createCollection(page, name);

  const activity = page.getByRole('region', { name: 'Recent changes' });
  await expect(activity.getByRole('row', { name: /collection\.create/ })).toHaveCount(1);
});

test('switching collections keeps the page on screen instead of flashing placeholders', async ({ page }) => {
  const first = unique('ui-switch-a');
  const second = unique('ui-switch-b');
  await createCollection(page, first, { Start: '100' });
  await createCollection(page, second, { Start: '200' });
  await expect(stat(page, 'Next ID')).toHaveText('200');
  // Slow the next load down so a placeholder, if any, is on screen long enough to see.
  await page.route('**/v1/collections/*', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 300));
    await route.continue();
  });
  await page.evaluate(() => {
    const seen = { placeholders: 0 };
    (window as unknown as { seen: typeof seen }).seen = seen;
    new MutationObserver(() => {
      if (document.querySelector('main [data-slot="skeleton"]')) seen.placeholders += 1;
    }).observe(document.body, { childList: true, subtree: true });
  });

  await page.getByRole('navigation', { name: 'Collections', exact: true }).getByRole('link', { name: first }).click();

  await expect(stat(page, 'Next ID')).toHaveText('100');
  expect(await page.evaluate(() => (window as unknown as { seen: { placeholders: number } }).seen.placeholders)).toBe(0);
});
