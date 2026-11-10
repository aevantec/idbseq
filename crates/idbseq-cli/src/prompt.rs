//! Interactive input: confirmations and the admin password.

use std::io::{self, BufRead, IsTerminal, Write};

use crate::error::CliError;

/// Source of interactive answers, replaceable in tests.
pub trait Prompt {
    /// Shows `message` and returns the line the user typed (without the newline).
    ///
    /// # Errors
    ///
    /// [`CliError::Aborted`] at end of input, [`CliError::Io`] when reading fails.
    fn line(&mut self, message: &str) -> Result<String, CliError>;
    /// Reads a secret without echo when possible.
    ///
    /// # Errors
    ///
    /// As [`Prompt::line`].
    fn secret(&mut self, message: &str) -> Result<String, CliError>;
}

/// Asks the user to type `expected` to confirm a destructive action.
///
/// # Errors
///
/// [`CliError::Aborted`] unless the answer is exactly `expected`.
pub fn confirm(prompt: &mut dyn Prompt, action: &str, expected: &str) -> Result<(), CliError> {
    let answer = prompt.line(&format!("{action}\nType `{expected}` to confirm: "))?;
    if answer.trim() == expected {
        Ok(())
    } else {
        Err(CliError::Aborted)
    }
}

fn read_stdin_line() -> Result<String, CliError> {
    let mut line = String::new();
    let read = io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|error| CliError::Io(format!("cannot read standard input: {error}")))?;
    if read == 0 {
        return Err(CliError::Aborted);
    }
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

/// Prompts on stderr and reads from stdin.
#[derive(Debug, Default)]
pub struct Terminal;

impl Prompt for Terminal {
    fn line(&mut self, message: &str) -> Result<String, CliError> {
        let mut stderr = io::stderr();
        // A prompt that cannot be shown is not fatal; the answer still decides.
        let _ = write!(stderr, "{message}").and_then(|()| stderr.flush());
        read_stdin_line()
    }

    fn secret(&mut self, message: &str) -> Result<String, CliError> {
        if io::stdin().is_terminal() {
            rpassword::prompt_password(message)
                .map_err(|error| CliError::Io(format!("cannot read the password: {error}")))
        } else {
            read_stdin_line()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Answer(&'static str);

    impl Prompt for Answer {
        fn line(&mut self, _: &str) -> Result<String, CliError> {
            Ok(self.0.to_owned())
        }
        fn secret(&mut self, _: &str) -> Result<String, CliError> {
            Ok(self.0.to_owned())
        }
    }

    #[test]
    fn confirms_only_the_exact_name() {
        assert_eq!(
            confirm(&mut Answer(" orders \n"), "Delete?", "orders"),
            Ok(())
        );
        assert_eq!(
            confirm(&mut Answer("y"), "Delete?", "orders"),
            Err(CliError::Aborted)
        );
    }
}
