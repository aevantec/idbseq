use std::time::{Duration, SystemTime};

use super::*;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_hours(500_000)
}

#[test]
fn simultaneous_attempts_count_before_their_passwords_are_checked() {
    let mut lockouts = Lockouts::default();

    // Five attempts begin at once; none has finished checking its password.
    let begun = (0..MAX_FAILED_LOGINS)
        .map(|_| lockouts.begin("203.0.113.7", now()))
        .collect::<Vec<_>>();

    assert!(begun.iter().all(Result::is_ok));
    assert_eq!(lockouts.begin("203.0.113.7", now()), Err(Locked));
}

#[test]
fn one_client_failing_does_not_lock_out_another() {
    let mut lockouts = Lockouts::default();
    for _ in 0..MAX_FAILED_LOGINS {
        lockouts.begin("203.0.113.7", now()).expect("begin");
    }

    assert_eq!(lockouts.begin("203.0.113.7", now()), Err(Locked));
    assert_eq!(lockouts.begin("198.51.100.20", now()), Ok(()));
}

#[test]
fn a_success_forgets_that_clients_failures() {
    let mut lockouts = Lockouts::default();
    for _ in 0..MAX_FAILED_LOGINS - 1 {
        lockouts.begin("203.0.113.7", now()).expect("begin");
    }
    lockouts
        .begin("203.0.113.7", now())
        .expect("the right password");

    lockouts.succeeded("203.0.113.7", now());

    for _ in 0..MAX_FAILED_LOGINS {
        assert_eq!(lockouts.begin("203.0.113.7", now()), Ok(()));
    }
}

#[test]
fn a_lock_ends_after_the_lockout_duration() {
    let mut lockouts = Lockouts::default();
    for _ in 0..MAX_FAILED_LOGINS {
        lockouts.begin("203.0.113.7", now()).expect("begin");
    }

    let still_locked = lockouts.begin(
        "203.0.113.7",
        now() + LOCKOUT_DURATION - Duration::from_secs(1),
    );
    let unlocked = lockouts.begin("203.0.113.7", now() + LOCKOUT_DURATION);

    assert_eq!(still_locked, Err(Locked));
    assert_eq!(unlocked, Ok(()));
}

#[test]
fn failures_older_than_the_window_do_not_count() {
    let mut lockouts = Lockouts::default();
    for _ in 0..MAX_FAILED_LOGINS - 1 {
        lockouts.begin("203.0.113.7", now()).expect("begin");
    }

    let later = now() + LOCKOUT_WINDOW + Duration::from_secs(1);

    for _ in 0..MAX_FAILED_LOGINS - 1 {
        assert_eq!(lockouts.begin("203.0.113.7", later), Ok(()));
    }
}

#[test]
fn guessing_spread_over_many_clients_hits_the_global_brake() {
    let mut lockouts = Lockouts::default();
    for client in 0..MAX_FAILED_LOGINS_GLOBAL {
        lockouts
            .begin(&format!("10.0.{}.{}", client / 250, client % 250), now())
            .expect("begin");
    }

    assert_eq!(lockouts.begin("198.51.100.20", now()), Err(Locked));
    assert_eq!(
        lockouts.begin("198.51.100.20", now() + LOCKOUT_DURATION),
        Ok(())
    );
}

#[test]
fn clearing_unlocks_everyone() {
    let mut lockouts = Lockouts::default();
    for _ in 0..MAX_FAILED_LOGINS {
        lockouts.begin("203.0.113.7", now()).expect("begin");
    }

    lockouts.clear();

    assert_eq!(lockouts.begin("203.0.113.7", now()), Ok(()));
}

#[test]
fn idle_clients_are_forgotten() {
    let mut lockouts = Lockouts::default();
    lockouts.begin("203.0.113.7", now()).expect("begin");

    lockouts
        .begin(
            "198.51.100.20",
            now() + LOCKOUT_WINDOW + Duration::from_secs(1),
        )
        .expect("begin");

    assert_eq!(lockouts.clients.len(), 1);
}

#[test]
fn clients_are_keyed_by_address_and_ipv6_by_its_64_network() {
    assert_eq!(client_key(Some("203.0.113.7")), "203.0.113.7");
    assert_eq!(client_key(Some("::ffff:203.0.113.7")), "203.0.113.7");
    assert_eq!(
        client_key(Some("2001:db8:1:2:aaaa::1")),
        client_key(Some("2001:db8:1:2:bbbb::9"))
    );
    assert_eq!(client_key(Some("2001:db8:1:2::1")), "2001:db8:1:2::/64");
    assert_ne!(
        client_key(Some("2001:db8:1:2::1")),
        client_key(Some("2001:db8:1:3::1"))
    );
    assert_eq!(client_key(None), "unknown");
    assert_eq!(client_key(Some("not an address")), "unknown");
}
