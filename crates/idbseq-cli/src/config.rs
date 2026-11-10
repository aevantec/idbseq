//! `~/.config/idbseq/config.toml` and the stored admin session.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CliError;

pub const CONFIG_FILE: &str = "config.toml";
pub const SESSION_FILE: &str = "session.toml";

/// Settings read from `config.toml`; flags and environment variables win.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
pub struct Config {
    pub url: Option<String>,
    pub key: Option<String>,
}

/// The admin session written by `idbseq login`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Server the token belongs to; the token is only sent there.
    pub url: String,
    pub token: String,
    pub expires_at: String,
}

/// Resolves the config directory: `$IDBSEQ_CONFIG_DIR`, else
/// `$XDG_CONFIG_HOME/idbseq`, else `$HOME/.config/idbseq`.
pub fn config_dir(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let non_empty = |name: &str| env(name).filter(|value| !value.is_empty());
    if let Some(dir) = non_empty("IDBSEQ_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(base) = non_empty("XDG_CONFIG_HOME") {
        return Some(Path::new(&base).join("idbseq"));
    }
    non_empty("HOME")
        .or_else(|| non_empty("USERPROFILE"))
        .map(|home| Path::new(&home).join(".config").join("idbseq"))
}

fn read_optional(path: &Path) -> Result<Option<String>, CliError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CliError::Io(format!(
            "cannot read {}: {error}",
            path.display()
        ))),
    }
}

fn parse_toml<T: for<'de> Deserialize<'de>>(path: &Path, text: &str) -> Result<T, CliError> {
    toml::from_str(text)
        .map_err(|error| CliError::Usage(format!("invalid {}: {error}", path.display())))
}

/// Reads `config.toml`; a missing file is an empty config.
///
/// # Errors
///
/// [`CliError::Io`] when the file cannot be read, [`CliError::Usage`] when it is invalid.
pub fn load_config(dir: &Path) -> Result<Config, CliError> {
    let path = dir.join(CONFIG_FILE);
    read_optional(&path)?.map_or_else(|| Ok(Config::default()), |text| parse_toml(&path, &text))
}

/// Reads the stored session, if any.
///
/// # Errors
///
/// [`CliError::Io`] when the file cannot be read, [`CliError::Usage`] when it is invalid.
pub fn load_session(dir: &Path) -> Result<Option<Session>, CliError> {
    let path = dir.join(SESSION_FILE);
    read_optional(&path)?
        .map(|text| parse_toml(&path, &text))
        .transpose()
}

fn io_error(path: &Path, error: &io::Error) -> CliError {
    CliError::Io(format!("cannot write {}: {error}", path.display()))
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

/// Writes the session file readable by the owner only (0600), atomically.
///
/// # Errors
///
/// [`CliError::Io`] when the directory or file cannot be written.
pub fn save_session(dir: &Path, session: &Session) -> Result<(), CliError> {
    let path = dir.join(SESSION_FILE);
    let text = toml::to_string(session).map_err(|error| CliError::Io(error.to_string()))?;
    create_private_dir(dir).map_err(|error| io_error(dir, &error))?;
    let temp = dir.join(format!(".{SESSION_FILE}.{}", std::process::id()));
    let written = create_private_file(&temp)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())
                .and_then(|()| file.sync_all())
        })
        .and_then(|()| fs::rename(&temp, &path));
    if let Err(error) = written {
        // Best effort: the temp file may not exist if creating it failed.
        let _ = fs::remove_file(&temp);
        return Err(io_error(&path, &error));
    }
    Ok(())
}

/// Deletes the session file. Returns whether there was one.
///
/// # Errors
///
/// [`CliError::Io`] when the file exists but cannot be removed.
pub fn remove_session(dir: &Path) -> Result<bool, CliError> {
    let path = dir.join(SESSION_FILE);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(CliError::Io(format!(
            "cannot remove {}: {error}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| {
            owned
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    fn session() -> Session {
        Session {
            url: "http://127.0.0.1:7878".into(),
            token: "tok".into(),
            expires_at: "2026-11-10T08:30:00Z".into(),
        }
    }

    #[test]
    fn config_dir_prefers_the_explicit_override() {
        let env = env_of(&[
            ("IDBSEQ_CONFIG_DIR", "/x"),
            ("XDG_CONFIG_HOME", "/y"),
            ("HOME", "/h"),
        ]);
        assert_eq!(config_dir(env), Some(PathBuf::from("/x")));
    }

    #[test]
    fn config_dir_uses_xdg_then_home() {
        let xdg = env_of(&[("XDG_CONFIG_HOME", "/y"), ("HOME", "/h")]);
        assert_eq!(config_dir(xdg), Some(PathBuf::from("/y/idbseq")));
        let home = env_of(&[("XDG_CONFIG_HOME", ""), ("HOME", "/h")]);
        assert_eq!(config_dir(home), Some(PathBuf::from("/h/.config/idbseq")));
        assert_eq!(config_dir(env_of(&[])), None);
    }

    #[test]
    fn a_missing_config_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_config(dir.path()).unwrap(), Config::default());
        assert_eq!(load_session(dir.path()).unwrap(), None);
    }

    #[test]
    fn reads_url_and_key_from_config() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE),
            "url = \"http://ids:1\"\nkey = \"idb_a_b\"\n",
        )
        .unwrap();

        let config = load_config(dir.path()).unwrap();

        assert_eq!(config.url.as_deref(), Some("http://ids:1"));
        assert_eq!(config.key.as_deref(), Some("idb_a_b"));
    }

    #[test]
    fn an_invalid_config_is_a_usage_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(CONFIG_FILE), "url = [").unwrap();

        assert!(matches!(load_config(dir.path()), Err(CliError::Usage(_))));
    }

    #[test]
    fn saves_loads_and_removes_the_session() {
        // Arrange
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("nested").join("idbseq");

        // Act
        save_session(&dir, &session()).unwrap();
        let loaded = load_session(&dir).unwrap();
        let removed = remove_session(&dir).unwrap();

        // Assert
        assert_eq!(loaded, Some(session()));
        assert!(removed);
        assert!(!remove_session(&dir).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn the_session_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("idbseq");

        save_session(&dir, &session()).unwrap();

        let file_mode = fs::metadata(dir.join(SESSION_FILE))
            .unwrap()
            .permissions()
            .mode();
        let dir_mode = fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(file_mode & 0o777, 0o600);
        assert_eq!(dir_mode & 0o777, 0o700);
    }

    #[test]
    fn saving_into_an_unwritable_place_is_an_io_error() {
        let root = tempfile::tempdir().unwrap();
        let blocker = root.path().join("file");
        fs::write(&blocker, "").unwrap();

        let result = save_session(&blocker.join("idbseq"), &session());

        assert!(matches!(result, Err(CliError::Io(_))));
    }
}
