//! Server configuration: `idbseq.toml`, then environment overrides (US-0014).
//!
//! Lookup: the file named by `IDBSEQ_CONFIG`, else `./idbseq.toml` if present,
//! else defaults. Then `IDBSEQ_BIND`, `IDBSEQ_DATA_DIR` and `IDBSEQ_LOG` override.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_BIND: &str = "127.0.0.1:7878";
pub const DEFAULT_CONFIG_FILE: &str = "idbseq.toml";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    /// `tracing` filter, e.g. `info` or `idbseq_server=debug`.
    pub log: String,
    pub tls: Option<TlsConfig>,
    pub limits: LimitsConfig,
    pub rate_limit: RateLimitConfig,
    pub auth: AuthConfig,
    pub group_commit: GroupCommitSettings,
    pub audit: AuditConfig,
    pub metrics: MetricsConfig,
    /// Serve Swagger UI at `/docs` (loads assets from a CDN). Off by default.
    pub swagger_ui: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    pub cert: PathBuf,
    pub key: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsConfig {
    pub body_bytes: usize,
    pub request_timeout_secs: u64,
    pub max_batch: u32,
    pub idempotency_ttl_hours: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimitConfig {
    /// Sustained requests per second per credential; 0 disables rate limiting.
    pub per_second: u32,
    pub burst: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuthConfig {
    pub session_ttl_minutes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GroupCommitSettings {
    pub max_batch: usize,
    pub max_wait_us: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuditConfig {
    pub retention_days: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MetricsConfig {
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: DEFAULT_BIND
                .parse()
                .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], 7878))),
            data_dir: PathBuf::from("data"),
            log: "info".into(),
            tls: None,
            limits: LimitsConfig::default(),
            rate_limit: RateLimitConfig::default(),
            auth: AuthConfig::default(),
            group_commit: GroupCommitSettings::default(),
            audit: AuditConfig::default(),
            metrics: MetricsConfig::default(),
            swagger_ui: false,
        }
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            body_bytes: 16 * 1024,
            request_timeout_secs: 10,
            max_batch: idbseq_core::engine::DEFAULT_MAX_BATCH,
            idempotency_ttl_hours: 24,
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            per_second: 10_000,
            burst: 20_000,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_ttl_minutes: 15,
        }
    }
}

impl Default for GroupCommitSettings {
    fn default() -> Self {
        Self {
            max_batch: 1024,
            max_wait_us: 0,
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            retention_days: 365,
        }
    }
}

impl LimitsConfig {
    #[must_use]
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs.max(1))
    }
}

impl AuthConfig {
    #[must_use]
    pub fn session_ttl(&self) -> Duration {
        Duration::from_secs(self.session_ttl_minutes.max(1) * 60)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {reason}")]
    Read { path: PathBuf, reason: String },
    #[error("invalid config file {path}: {reason}")]
    Parse { path: PathBuf, reason: String },
    #[error("invalid {var}: {reason}")]
    Env { var: &'static str, reason: String },
}

impl Config {
    /// Load the config file (if any) and apply environment overrides.
    ///
    /// # Errors
    ///
    /// Unreadable or invalid file, or an invalid environment value.
    pub fn load(env: &dyn Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let explicit = env("IDBSEQ_CONFIG").map(PathBuf::from);
        let path = explicit.clone().or_else(|| {
            let default = PathBuf::from(DEFAULT_CONFIG_FILE);
            default.exists().then_some(default)
        });
        let base = match path {
            Some(path) => Self::from_file(&path)?,
            None => Self::default(),
        };
        base.with_env(env)
    }

    fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError::Read {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
        toml::from_str(&text).map_err(|e| ConfigError::Parse {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })
    }

    fn with_env(self, env: &dyn Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let bind = match env("IDBSEQ_BIND") {
            Some(value) => {
                value
                    .parse()
                    .map_err(|e: std::net::AddrParseError| ConfigError::Env {
                        var: "IDBSEQ_BIND",
                        reason: e.to_string(),
                    })?
            }
            None => self.bind,
        };
        Ok(Self {
            bind,
            data_dir: env("IDBSEQ_DATA_DIR").map_or(self.data_dir.clone(), PathBuf::from),
            log: env("IDBSEQ_LOG").unwrap_or_else(|| self.log.clone()),
            ..self
        })
    }

    #[must_use]
    pub fn group_commit(&self) -> idbseq_core::store::GroupCommitConfig {
        idbseq_core::store::GroupCommitConfig {
            max_batch: self.group_commit.max_batch,
            max_wait: Duration::from_micros(self.group_commit.max_wait_us),
        }
    }

    #[must_use]
    pub fn engine_limits(&self) -> idbseq_core::engine::Limits {
        idbseq_core::engine::Limits {
            max_batch: self.limits.max_batch,
            idempotency_ttl: Duration::from_secs(self.limits.idempotency_ttl_hours.max(1) * 3600),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn defaults_bind_to_loopback_7878() {
        let config = Config::load(&env_of(&[])).expect("defaults");

        assert_eq!(config.bind.to_string(), DEFAULT_BIND);
        assert_eq!(config.limits.body_bytes, 16 * 1024);
        assert!(config.tls.is_none());
        assert!(!config.metrics.enabled);
    }

    #[test]
    fn file_values_are_overridden_by_environment() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("idbseq.toml");
        std::fs::write(
            &file,
            "bind = \"0.0.0.0:9000\"\ndata_dir = \"/var/lib/idbseq\"\nlog = \"debug\"\n[limits]\nmax_batch = 500\n[metrics]\nenabled = true\n",
        )
        .expect("write");
        let path = file.to_string_lossy().into_owned();

        let config = Config::load(&env_of(&[
            ("IDBSEQ_CONFIG", &path),
            ("IDBSEQ_BIND", "127.0.0.1:9100"),
            ("IDBSEQ_LOG", "warn"),
        ]))
        .expect("load");

        assert_eq!(config.bind.to_string(), "127.0.0.1:9100");
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/idbseq"));
        assert_eq!(config.log, "warn");
        assert_eq!(config.limits.max_batch, 500);
        assert!(config.metrics.enabled);
    }

    #[test]
    fn invalid_values_are_reported_with_their_source() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("bad.toml");
        std::fs::write(&file, "unknown_key = 1\n").expect("write");
        let path = file.to_string_lossy().into_owned();

        let bad_file = Config::load(&env_of(&[("IDBSEQ_CONFIG", &path)]));
        let bad_env = Config::load(&env_of(&[("IDBSEQ_BIND", "not-an-address")]));
        let missing = Config::load(&env_of(&[("IDBSEQ_CONFIG", "/nonexistent/idbseq.toml")]));

        assert!(matches!(bad_file, Err(ConfigError::Parse { .. })));
        assert!(matches!(
            bad_env,
            Err(ConfigError::Env {
                var: "IDBSEQ_BIND",
                ..
            })
        ));
        assert!(matches!(missing, Err(ConfigError::Read { .. })));
    }
}
