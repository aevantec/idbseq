import { PASSWORD, expect, rail, signIn, test } from './helpers';

test('rejects a wrong password with a clear message', async ({ page }) => {
  await page.goto('./');

  await page.getByLabel('Password').fill(`${PASSWORD} wrong`);
  await page.getByRole('button', { name: 'Sign in' }).click();

  await expect(page.getByRole('alert')).toContainText('Wrong password');
  await expect(page.getByRole('heading', { name: 'idbseq console' })).toBeVisible();
});

test('signs in, shows the session countdown, and signs out', async ({ page }) => {
  await signIn(page);
  await expect(page.locator('.countdown')).toHaveText(/^\d+:\d\d$/);

  await rail(page).getByRole('button', { name: 'Sign out' }).click();

  await expect(page.getByText('Signed out.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
});

test('keeps the session in memory only: nothing stored, reload signs out', async ({ page, context }) => {
  await signIn(page);

  const stored = await page.evaluate(() => ({ local: localStorage.length, session: sessionStorage.length }));
  expect(stored).toEqual({ local: 0, session: 0 });
  expect(await context.cookies()).toEqual([]);

  await page.reload();
  await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
});

test('a revoked session sends the console back to the login screen', async ({ page, request }) => {
  await page.goto('./');
  await page.getByLabel('Password').fill(PASSWORD);
  const login = page.waitForResponse((response) => response.url().endsWith('/v1/auth/login'));
  await page.getByRole('button', { name: 'Sign in' }).click();
  const { data } = (await (await login).json()) as { data: { token: string } };
  await expect(page.getByRole('heading', { name: 'Collections', level: 2 })).toBeVisible();

  const logout = await request.post('/v1/auth/logout', { headers: { authorization: `Bearer ${data.token}` } });
  expect(logout.ok()).toBe(true);
  await rail(page).getByRole('link', { name: 'API keys' }).click();

  await expect(page.getByText('Your session ended. Sign in again.')).toBeVisible();
});

test('a deep link asks for sign-in first, then opens the page', async ({ page }) => {
  await page.goto('./#/audit');
  await page.getByLabel('Password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Sign in' }).click();

  await expect(page.getByRole('heading', { name: 'Audit log', level: 1 })).toBeVisible();
});

test('serves the page under a strict Content-Security-Policy', async ({ page }) => {
  const response = await page.goto('./');

  const csp = response?.headers()['content-security-policy'] ?? '';
  expect(csp).toContain("default-src 'none'");
  expect(csp).toContain("script-src 'self'");
  expect(csp).not.toContain('unsafe-inline');
});
