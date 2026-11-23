// A console link opened on a real server (not `idbseqd console`) must not sign
// in with whatever token it carries, an API key included.
import { expect, test } from './helpers';

test('a console link on a server refuses to sign in and points to the password', async ({ page }) => {
  await page.goto('./#/local?token=idb_some_api_key&file=app.idbseq');

  await expect(page.getByText('This link only works with idbseqd console')).toBeVisible();
  expect(page.url()).not.toContain('token=');
  await page.getByRole('link', { name: 'Sign in with the password' }).click();
  await expect(page.getByLabel('Password')).toBeVisible();
});
