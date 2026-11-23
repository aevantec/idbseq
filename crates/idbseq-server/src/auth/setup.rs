//! First-run setup from the console (US-0047).
//!
//! When the console is on and no admin exists, `idbseqd` prints a one-time
//! link. Opening it lets whoever can read the server's output choose the admin
//! password in the browser, like `idbseqd admin init` does in a terminal.
//! Only the token's hash is kept, in memory: a restart issues a new link.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use super::secrets::{constant_time_eq, random_token, sha256_hex};

/// How long a setup link stays valid.
pub const SETUP_TTL: Duration = Duration::from_mins(30);

#[derive(Debug)]
struct Pending {
    hash: String,
    expires_at: SystemTime,
}

/// Why a setup attempt failed.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SetupError {
    /// No link is pending, or the token is wrong or expired (one message for all).
    #[error("the setup link is invalid or has expired; restart idbseqd for a new one")]
    InvalidToken,
}

/// The pending setup link, if any.
#[derive(Debug, Default)]
pub struct Setup {
    pending: Mutex<Option<Pending>>,
}

impl Setup {
    /// Start a setup window and return the token for the link. Replaces any earlier one.
    pub fn issue(&self, now: SystemTime) -> String {
        let token = random_token();
        *self.lock() = Some(Pending {
            hash: sha256_hex(&token),
            expires_at: now + SETUP_TTL,
        });
        token
    }

    /// Whether a link is waiting to be used.
    #[must_use]
    pub fn is_pending(&self, now: SystemTime) -> bool {
        self.lock().as_ref().is_some_and(|p| p.expires_at > now)
    }

    /// Run `create` if `token` is the pending, unexpired link. The link is used up
    /// only when `create` succeeds, so a rejected password can be retried. Attempts
    /// are serialised, so two browsers cannot both create the admin.
    ///
    /// # Errors
    ///
    /// `InvalidToken`, or whatever `create` returns (wrapped in `Ok(Err(..))`).
    pub fn complete<T, E>(
        &self,
        token: &str,
        now: SystemTime,
        create: impl FnOnce() -> Result<T, E>,
    ) -> Result<Result<T, E>, SetupError> {
        let mut pending = self.lock();
        let is_valid = pending
            .as_ref()
            .is_some_and(|p| p.expires_at > now && constant_time_eq(&p.hash, &sha256_hex(token)));
        if !is_valid {
            return Err(SetupError::InvalidToken);
        }
        let result = create();
        if result.is_ok() {
            *pending = None;
        }
        Ok(result)
    }

    /// Forget the pending link (an admin was created another way).
    pub fn cancel(&self) {
        *self.lock() = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Pending>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The console URL that completes setup. The token goes in the fragment, which
/// browsers never send to the server, so it stays out of access logs and
/// `Referer` headers. A wildcard bind address is shown as loopback.
#[must_use]
pub fn link(bind: SocketAddr, is_tls: bool, token: &str) -> String {
    let scheme = if is_tls { "https" } else { "http" };
    let host = match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), bind.port())
        }
        IpAddr::V6(ip) if ip.is_unspecified() => {
            SocketAddr::new(Ipv6Addr::LOCALHOST.into(), bind.port())
        }
        _ => bind,
    };
    format!("{scheme}://{host}/ui/#/setup?token={token}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unnecessary_wraps)] // has the shape of a `create` callback
    fn ok() -> Result<(), ()> {
        Ok(())
    }

    #[test]
    fn a_token_completes_setup_once() {
        let setup = Setup::default();
        let now = SystemTime::now();
        let token = setup.issue(now);

        assert!(setup.is_pending(now));
        assert_eq!(setup.complete(&token, now, ok), Ok(Ok(())));
        assert!(!setup.is_pending(now));
        assert_eq!(
            setup.complete(&token, now, ok),
            Err(SetupError::InvalidToken)
        );
    }

    #[test]
    fn wrong_expired_and_missing_tokens_are_refused_without_running_create() {
        let setup = Setup::default();
        let now = SystemTime::now();
        let refuse = || -> Result<(), ()> { panic!("create must not run") };

        assert_eq!(
            setup.complete("anything", now, refuse),
            Err(SetupError::InvalidToken)
        );
        let token = setup.issue(now);
        assert_eq!(
            setup.complete("wrong", now, refuse),
            Err(SetupError::InvalidToken)
        );
        let later = now + SETUP_TTL;
        assert!(!setup.is_pending(later));
        assert_eq!(
            setup.complete(&token, later, refuse),
            Err(SetupError::InvalidToken)
        );
    }

    #[test]
    fn a_failed_create_keeps_the_link_and_a_new_issue_replaces_it() {
        let setup = Setup::default();
        let now = SystemTime::now();
        let first = setup.issue(now);

        assert_eq!(
            setup.complete(&first, now, || Err::<(), _>("weak")),
            Ok(Err("weak"))
        );
        assert!(setup.is_pending(now));
        let second = setup.issue(now);
        assert_eq!(
            setup.complete(&first, now, ok),
            Err(SetupError::InvalidToken)
        );
        setup.cancel();
        assert_eq!(
            setup.complete(&second, now, ok),
            Err(SetupError::InvalidToken)
        );
    }

    #[test]
    fn links_use_the_bind_address_with_wildcards_shown_as_loopback() {
        let at = |s: &str| s.parse::<SocketAddr>().expect("addr");

        assert_eq!(
            link(at("127.0.0.1:7878"), false, "t"),
            "http://127.0.0.1:7878/ui/#/setup?token=t"
        );
        assert_eq!(
            link(at("0.0.0.0:9000"), true, "t"),
            "https://127.0.0.1:9000/ui/#/setup?token=t"
        );
        assert_eq!(
            link(at("[::]:7878"), false, "t"),
            "http://[::1]:7878/ui/#/setup?token=t"
        );
        assert_eq!(
            link(at("10.0.0.5:7878"), false, "t"),
            "http://10.0.0.5:7878/ui/#/setup?token=t"
        );
    }
}
