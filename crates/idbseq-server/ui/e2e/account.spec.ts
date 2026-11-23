import type { APIRequestContext } from '@playwright/test';

import { PASSWORD, expect, rail, signIn, test } from './helpers';

const NEW_PASSWORD = `${PASSWORD} (changed)`;

/** Put the original password back, whatever state the test left it in. */
async function restore(request: APIRequestContext): Promise<void> {
  const login = await request.post('/v1/auth/login', { data: { username: 'admin', password: NEW_PASSWORD } });
  if (!login.ok()) return; // never changed
  const { data } = (await login.json()) as { data: { token: string } };
  const reset = await request.post('/v1/admin/password', {
    headers: { authorization: `Bearer ${data.token}` },
    data: { current_password: NEW_PASSWORD, new_password: PASSWORD },
  });
  expect(reset.ok(), 'restore the original admin password').toBe(true);
}

test('rejects a wrong current password without signing out', async ({ page }) => {
  await signIn(page);
  await rail(page).getByRole('button', { name: 'Account' }).click();
  const panel = page.getByRole('dialog', { name: 'Account' });

  await panel.getByLabel('Current password').fill('not the password');
  await panel.getByLabel('New password', { exact: true }).fill(NEW_PASSWORD);
  await panel.getByLabel('Confirm new password').fill(NEW_PASSWORD);
  await panel.getByRole('button', { name: 'Change password' }).click();

  await expect(panel.getByRole('alert')).toContainText('The current password is incorrect');
  await page.keyboard.press('Escape');
  await rail(page).getByRole('link', { name: 'API keys' }).click();
  await expect(page.getByRole('heading', { name: 'API keys', level: 1 })).toBeVisible();
});

test('checks the new password before sending it', async ({ page }) => {
  await signIn(page);
  await rail(page).getByRole('button', { name: 'Account' }).click();
  const panel = page.getByRole('dialog', { name: 'Account' });
  const sent = page.waitForRequest('**/v1/admin/password', { timeout: 1000 }).then(() => true, () => false);

  await panel.getByLabel('Current password').fill(PASSWORD);
  await panel.getByLabel('New password', { exact: true }).fill('short');
  await panel.getByLabel('Confirm new password').fill('different');
  await panel.getByRole('button', { name: 'Change password' }).click();

  await expect(panel.getByRole('alert')).toContainText('at least 8 characters');
  await expect(panel.getByRole('alert')).toContainText('do not match');
  expect(await sent).toBe(false);
});

test('changes the password, signs out, and the new password works', async ({ page, request }) => {
  try {
    await signIn(page);
    await rail(page).getByRole('button', { name: 'Account' }).click();
    const panel = page.getByRole('dialog', { name: 'Account' });

    await panel.getByLabel('Current password').fill(PASSWORD);
    await panel.getByLabel('New password', { exact: true }).fill(NEW_PASSWORD);
    await panel.getByLabel('Confirm new password').fill(NEW_PASSWORD);
    await panel.getByRole('button', { name: 'Change password' }).click();

    await expect(page.getByText('Password changed. Sign in with the new password.')).toBeVisible();
    await page.getByLabel('Password').fill(NEW_PASSWORD);
    await page.getByRole('button', { name: 'Sign in' }).click();
    await expect(page.getByRole('heading', { name: 'Collections', level: 2 })).toBeVisible();

    await rail(page).getByRole('link', { name: 'Audit log' }).click();
    await page.getByLabel('Action').selectOption('admin.password_change');
    await expect(page.getByRole('row', { name: /admin\.password_change/ }).first()).toBeVisible();
  } finally {
    await restore(request);
  }
});
