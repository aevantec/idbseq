// Checks for choosing an admin password, matching the server's rules.

/** The server's minimum (`MIN_PASSWORD_LEN`), counted in characters. */
export const MIN_PASSWORD_LEN = 8;

export interface PasswordChange {
  current: string;
  next: string;
  confirm: string;
}

/** Problems with a new password and its confirmation (setup and change). */
export function validateNewPassword(next: string, confirm: string): string[] {
  const errors: string[] = [];
  // Rust counts `chars()` (code points), not UTF-16 units.
  if ([...next].length < MIN_PASSWORD_LEN) {
    errors.push(`The new password must be at least ${MIN_PASSWORD_LEN} characters.`);
  }
  if (next !== confirm) errors.push('The new passwords do not match.');
  return errors;
}

/** Every problem with the change-password form, or an empty list when it can be sent. */
export function validatePasswordChange({ current, next, confirm }: PasswordChange): string[] {
  const errors = [
    ...(current === '' ? ['Enter your current password.'] : []),
    ...validateNewPassword(next, confirm),
  ];
  if (errors.length === 0 && next === current) {
    return ['The new password must differ from the current one.'];
  }
  return errors;
}
