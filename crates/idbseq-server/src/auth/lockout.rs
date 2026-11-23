//! Login lockout (US-0022).
//!
//! Failed logins are counted per client address, so a stranger who guesses
//! wrong locks out only themselves, never the admin signing in from elsewhere.
//! A global brake stops guessing spread over many addresses.
//!
//! An attempt counts as failed *before* its password is checked and is
//! forgotten when it succeeds. Checking the password is slow on purpose, so
//! counting afterwards would let a burst of simultaneous attempts all pass the
//! lock before the first of them was counted.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, SystemTime};

/// Failed logins one client may make within [`LOCKOUT_WINDOW`] before it is locked out.
pub const MAX_FAILED_LOGINS: usize = 5;
/// Failed logins from all clients together within [`LOCKOUT_WINDOW`] before every
/// login is refused: about 200 guesses an hour, against guessing spread over many
/// addresses.
pub const MAX_FAILED_LOGINS_GLOBAL: usize = 50;
pub const LOCKOUT_WINDOW: Duration = Duration::from_mins(15);
pub const LOCKOUT_DURATION: Duration = Duration::from_mins(15);

/// The client or every client is locked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locked;

/// Recent failures and the lock they caused, for one client or for everyone.
#[derive(Debug, Default)]
struct Counter {
    failures: VecDeque<SystemTime>,
    locked_until: Option<SystemTime>,
}

impl Counter {
    fn is_locked(&self, now: SystemTime) -> bool {
        self.locked_until.is_some_and(|until| now < until)
    }

    fn forget_old(&mut self, now: SystemTime) {
        let window_start = now
            .checked_sub(LOCKOUT_WINDOW)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        self.failures.retain(|at| *at > window_start);
    }

    fn add(&mut self, now: SystemTime, limit: usize) {
        self.forget_old(now);
        self.failures.push_back(now);
        if self.failures.len() >= limit {
            self.locked_until = Some(now + LOCKOUT_DURATION);
            self.failures.clear();
        }
    }

    fn forget(&mut self, at: SystemTime) {
        if let Some(index) = self.failures.iter().position(|failed| *failed == at) {
            self.failures.remove(index);
        }
    }

    fn is_idle(&mut self, now: SystemTime) -> bool {
        self.forget_old(now);
        self.failures.is_empty() && !self.is_locked(now)
    }
}

/// Lockout state for every client.
#[derive(Debug, Default)]
pub struct Lockouts {
    clients: HashMap<String, Counter>,
    everyone: Counter,
}

impl Lockouts {
    /// Start an attempt from `client` (see [`client_key`]), counted as failed
    /// until [`Lockouts::succeeded`].
    ///
    /// # Errors
    ///
    /// [`Locked`] if this client or every client is locked out; then nothing is
    /// counted.
    pub fn begin(&mut self, client: &str, now: SystemTime) -> Result<(), Locked> {
        // Every client entry holds a failure that also counts globally, so the
        // global brake bounds how many entries there can be.
        self.clients.retain(|_, counter| !counter.is_idle(now));
        let counter = self.clients.entry(client.to_owned()).or_default();
        if self.everyone.is_locked(now) || counter.is_locked(now) {
            return Err(Locked);
        }
        counter.add(now, MAX_FAILED_LOGINS);
        self.everyone.add(now, MAX_FAILED_LOGINS_GLOBAL);
        Ok(())
    }

    /// The attempt `client` began at `began_at` was right: forget its failures.
    pub fn succeeded(&mut self, client: &str, began_at: SystemTime) {
        self.clients.remove(client);
        self.everyone.forget(began_at);
    }

    /// Forget every failure and lock, after the password is changed or reset.
    pub fn clear(&mut self) {
        self.clients.clear();
        self.everyone = Counter::default();
    }
}

/// Which client a login comes from: its IP address, or its /64 network for IPv6,
/// where one machine usually holds a whole /64 and could rotate through it.
#[must_use]
pub fn client_key(ip: Option<&str>) -> String {
    let Some(ip) = ip.and_then(|ip| ip.parse::<IpAddr>().ok()) else {
        return "unknown".to_owned();
    };
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return v4.to_string();
            }
            let [a, b, c, d, ..] = v6.segments();
            format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
        }
    }
}

#[cfg(test)]
#[path = "lockout_tests.rs"]
mod tests;
