// `idbseqd console <file>`: the link signs in, server-only parts are hidden,
// and the code samples open the file. Run by scripts/e2e.sh ui, which starts
// the console and passes its link.
import { createCollection, expect, rail, stat, takeIds, test, unique } from './helpers';

const LINK = process.env.IDBSEQ_E2E_CONSOLE_LINK ?? '';

test.skip(LINK === '', 'Set IDBSEQ_E2E_CONSOLE_LINK (or run scripts/e2e.sh ui).');

test('the link opens the file with no sign-in, and leaves the token out of the address bar', async ({ page }) => {
  await page.goto(LINK);

  await expect(page.getByRole('heading', { name: 'Collections', level: 2 })).toBeVisible();
  expect(page.url()).not.toContain('token=');
  const nav = rail(page);
  await expect(nav.getByRole('link', { name: 'Collections' })).toBeVisible();
  await expect(nav.getByRole('link', { name: 'Status' })).toBeVisible();
  await expect(nav.getByRole('link', { name: 'API keys' })).toHaveCount(0);
  await expect(nav.getByRole('link', { name: 'Audit log' })).toHaveCount(0);
  await expect(nav.getByRole('button', { name: 'Sign out' })).toHaveCount(0);
  await expect(nav.getByLabel('File: app.idbseq')).toBeVisible();
});

test('collections work as on a server, and the code samples open the file', async ({ page }) => {
  await page.goto(LINK);
  const name = unique('local');

  await rail(page).getByRole('link', { name: 'Collections' }).click();
  await page.getByRole('button', { name: 'New collection' }).click();
  await expect(page.getByRole('dialog', { name: 'New collection' }).getByLabel('Protected')).toHaveCount(0); // no API keys to keep out
  await page.keyboard.press('Escape');
  await createCollection(page, name, { Start: '500' });
  await takeIds(page, '3');

  await expect(stat(page, 'Last ID')).toHaveText('502');
  await expect(page.getByRole('heading', { name: 'Recent changes' })).toHaveCount(0); // no audit log
  await page.getByRole('button', { name: 'API preview' }).click();
  const panel = page.getByRole('dialog', { name: 'API preview' });
  await expect(panel.getByText("IdbSeqDatabase.open('app.idbseq')").first()).toBeVisible();
  await expect(panel.getByRole('link', { name: 'Create a key' })).toHaveCount(0);
});

test('a wrong token says the link no longer works, and server pages stay out of reach', async ({ page }) => {
  const wrong = LINK.replace(/token=[^&]+/, 'token=not-the-token');

  await page.goto(wrong);

  await expect(page.getByText('This console link no longer works')).toBeVisible();
  await page.goto(`${new URL(LINK).origin}/ui/#/keys`);
  await expect(page.getByText('This console link no longer works')).toBeVisible();
});

test('a reload asks for the link again instead of a password that does not exist', async ({ page }) => {
  await page.goto(LINK);
  await expect(page.getByRole('heading', { name: 'Collections', level: 2 })).toBeVisible();

  await page.reload();

  await expect(page.getByText('Reloading the page signs you out')).toBeVisible();
  await expect(page.getByLabel('Password')).toHaveCount(0);
});
