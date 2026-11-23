import { PASSWORD, expect, test } from './helpers';

const LINK_TOKEN = 'a'.repeat(64);
const CHOSEN = 'chosen in the browser';

test('the setup link creates the admin and signs in', async ({ page, request }) => {
  // The shared server already has an admin, so answer setup with a real session.
  const login = await request.post('/v1/auth/login', { data: { username: 'admin', password: PASSWORD } });
  const session = (await login.json()) as unknown;
  let sent: unknown = null;
  await page.route('**/v1/auth/setup', async (route) => {
    sent = route.request().postDataJSON();
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(session) });
  });

  await page.goto(`./#/setup?token=${LINK_TOKEN}`);
  await expect(page.getByRole('heading', { name: 'Set up idbseq' })).toBeVisible();
  await page.getByLabel('Password', { exact: true }).fill(CHOSEN);
  await page.getByLabel('Confirm password').fill(CHOSEN);
  await page.getByRole('button', { name: 'Create admin and sign in' }).click();

  await expect(page.getByRole('heading', { name: 'Collections', level: 2 })).toBeVisible();
  await expect(page.getByText('Admin account created.')).toBeVisible();
  expect(sent).toEqual({ token: LINK_TOKEN, password: CHOSEN });
  expect(page.url()).not.toContain(LINK_TOKEN);
});

test('checks the password before sending it', async ({ page }) => {
  const sent = page.waitForRequest('**/v1/auth/setup', { timeout: 1000 }).then(() => true, () => false);
  await page.goto(`./#/setup?token=${LINK_TOKEN}`);

  await page.getByLabel('Password', { exact: true }).fill('short');
  await page.getByLabel('Confirm password').fill('other');
  await page.getByRole('button', { name: 'Create admin and sign in' }).click();

  await expect(page.getByRole('alert')).toContainText('at least 8 characters');
  await expect(page.getByRole('alert')).toContainText('do not match');
  expect(await sent).toBe(false);
});

test('a wrong or used link says how to get a new one', async ({ page }) => {
  await page.goto(`./#/setup?token=${LINK_TOKEN}`);

  await page.getByLabel('Password', { exact: true }).fill(CHOSEN);
  await page.getByLabel('Confirm password').fill(CHOSEN);
  await page.getByRole('button', { name: 'Create admin and sign in' }).click();

  await expect(page.getByRole('alert')).toContainText('invalid, already used or expired. Restart idbseqd');
});

test('without a token the page explains where the link comes from', async ({ page }) => {
  await page.goto('./#/setup');

  await expect(page.getByText('This page needs the setup link')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Create admin and sign in' })).toHaveCount(0);
});
