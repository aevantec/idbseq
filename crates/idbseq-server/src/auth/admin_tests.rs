use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::*;

const PASSWORD: &str = "correct horse battery";

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000)
}

fn admin() -> (AdminAuth, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = redb::Database::create(dir.path().join("t.redb")).expect("db");
    crate::db::init(&db).expect("init");
    let auth = AdminAuth::load(Arc::new(db), Duration::from_mins(15)).expect("load");
    (auth, dir)
}

/// Where the logins in these tests come from.
const CLIENT: &str = "127.0.0.1";

#[test]
fn init_creates_the_admin_once_and_enforces_password_length() {
    let (auth, _dir) = admin();

    let weak = auth.init("short", now());
    let created = auth.init(PASSWORD, now());
    let again = auth.init(PASSWORD, now());

    assert!(matches!(weak, Err(AdminError::WeakPassword)));
    assert!(created.is_ok());
    assert!(matches!(again, Err(AdminError::AlreadyInitialized)));
    assert!(auth.exists().expect("exists"));
}

#[test]
fn passwords_need_at_least_eight_characters_counted_as_characters() {
    let (auth, _dir) = admin();

    let seven = auth.init("1234567", now());
    // Eight characters but 32 bytes: counted as eight.
    let eight_emoji = auth.set_password(&"🔑".repeat(8), now());
    let eight = auth.set_password("12345678", now());

    assert_eq!(MIN_PASSWORD_LEN, 8);
    assert!(matches!(seven, Err(AdminError::WeakPassword)));
    assert!(eight_emoji.is_ok(), "{eight_emoji:?}");
    assert!(eight.is_ok(), "{eight:?}");
    assert!(auth.verify_password("12345678").expect("verify"));
}

#[test]
fn login_before_init_reports_not_initialized() {
    let (auth, _dir) = admin();

    assert!(matches!(
        auth.login(CLIENT, ADMIN_USERNAME, PASSWORD, now()),
        Err(AdminError::NotInitialized)
    ));
}

#[test]
fn login_issues_a_session_that_expires() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");

    let session = auth
        .login(CLIENT, ADMIN_USERNAME, PASSWORD, now())
        .expect("login");

    assert_eq!(session.token.len(), 64);
    assert!(auth.session(&session.token, now()).is_some());
    assert!(auth.session(&session.token, session.expires_at).is_none());
    assert!(auth.session("not-a-token", now()).is_none());
}

#[test]
fn wrong_user_and_wrong_password_fail_the_same_way() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");

    let wrong_user = auth
        .login(CLIENT, "root", PASSWORD, now())
        .expect_err("wrong user");
    let wrong_password = auth
        .login(CLIENT, ADMIN_USERNAME, "wrong password!", now())
        .expect_err("wrong pw");

    assert_eq!(wrong_user.to_string(), wrong_password.to_string());
}

#[test]
fn five_failures_lock_the_account_for_fifteen_minutes() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    for _ in 0..MAX_FAILED_LOGINS {
        let _ = auth.login(CLIENT, ADMIN_USERNAME, "wrong password!", now());
    }

    let while_locked = auth.login(
        CLIENT,
        ADMIN_USERNAME,
        PASSWORD,
        now() + Duration::from_mins(14),
    );
    let after_lock = auth.login(CLIENT, ADMIN_USERNAME, PASSWORD, now() + LOCKOUT_DURATION);

    assert!(matches!(while_locked, Err(AdminError::Locked)));
    assert!(after_lock.is_ok());
}

#[test]
fn a_locked_out_client_does_not_lock_out_the_admin_elsewhere() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    for _ in 0..MAX_FAILED_LOGINS {
        let _ = auth.login("203.0.113.7", "x", "wrong password!", now());
    }

    let stranger = auth.login("203.0.113.7", ADMIN_USERNAME, PASSWORD, now());
    let admin_elsewhere = auth.login("198.51.100.20", ADMIN_USERNAME, PASSWORD, now());

    assert!(matches!(stranger, Err(AdminError::Locked)));
    assert!(admin_elsewhere.is_ok());
}

#[test]
fn setting_the_password_ends_every_lockout() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    for _ in 0..MAX_FAILED_LOGINS {
        let _ = auth.login(CLIENT, ADMIN_USERNAME, "wrong password!", now());
    }

    auth.set_password("a brand new password", now())
        .expect("reset");

    assert!(
        auth.login(CLIENT, ADMIN_USERNAME, "a brand new password", now())
            .is_ok()
    );
}

#[test]
fn failures_outside_the_window_do_not_count() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    for i in 0..u64::try_from(MAX_FAILED_LOGINS).expect("small") {
        let at = now() + Duration::from_mins(16 * i);
        let _ = auth.login(CLIENT, ADMIN_USERNAME, "wrong password!", at);
    }

    let later = now() + Duration::from_mins(16 * 5);
    assert!(auth.login(CLIENT, ADMIN_USERNAME, PASSWORD, later).is_ok());
}

#[test]
fn logout_and_password_change_revoke_sessions() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    let first = auth
        .login(CLIENT, ADMIN_USERNAME, PASSWORD, now())
        .expect("login");
    let second = auth
        .login(CLIENT, ADMIN_USERNAME, PASSWORD, now())
        .expect("login");
    let first_hash = auth.session(&first.token, now()).expect("live");

    auth.logout(&first_hash).expect("logout");
    let after_logout = auth.session(&first.token, now());
    auth.set_password("a brand new password", now())
        .expect("change");

    assert!(after_logout.is_none());
    assert!(auth.session(&second.token, now()).is_none());
    assert!(
        auth.verify_password("a brand new password")
            .expect("verify")
    );
    assert!(!auth.verify_password(PASSWORD).expect("verify"));
}

#[test]
fn sessions_survive_a_restart_and_expired_ones_are_purged() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Arc::new(redb::Database::create(dir.path().join("t.redb")).expect("db"));
    crate::db::init(&db).expect("init");
    let auth = AdminAuth::load(Arc::clone(&db), Duration::from_mins(15)).expect("load");
    auth.init(PASSWORD, now()).expect("init");
    let session = auth
        .login(CLIENT, ADMIN_USERNAME, PASSWORD, now())
        .expect("login");
    drop(auth);

    let reloaded = AdminAuth::load(db, Duration::from_mins(15)).expect("reload");
    let live = reloaded.session(&session.token, now()).is_some();
    let purged = reloaded.purge_expired(session.expires_at).expect("purge");

    assert!(live);
    assert_eq!(purged, 1);
    assert!(reloaded.session(&session.token, now()).is_none());
}

fn signed_in() -> (AdminAuth, Session, tempfile::TempDir) {
    let (auth, dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    let session = auth
        .login(CLIENT, ADMIN_USERNAME, PASSWORD, now())
        .expect("login");
    (auth, session, dir)
}

#[test]
fn refresh_slides_the_session_to_a_full_ttl_from_now() {
    let (auth, session, _dir) = signed_in();
    let hash = auth.session(&session.token, now()).expect("live");
    let later = now() + Duration::from_mins(10);

    let expires = auth.refresh(&hash, later).expect("refresh");

    assert_eq!(expires, Some(later + Duration::from_mins(15)));
    assert!(
        auth.session(&session.token, now() + Duration::from_mins(20))
            .is_some()
    );
    assert!(
        auth.session(&session.token, later + Duration::from_mins(15))
            .is_none()
    );
}

#[test]
fn refresh_never_goes_past_the_maximum_lifetime() {
    let (auth, session, _dir) = signed_in();
    let hash = auth.session(&session.token, now()).expect("live");
    let limit = now() + MAX_SESSION_LIFETIME;

    let mut at = now();
    while at + Duration::from_mins(10) < limit {
        at += Duration::from_mins(10);
        auth.refresh(&hash, at)
            .expect("refresh")
            .expect("still live");
    }
    let last = auth.refresh(&hash, at).expect("refresh");

    assert_eq!(MAX_SESSION_LIFETIME, Duration::from_hours(12));
    assert_eq!(last, Some(limit));
    assert!(auth.session(&session.token, limit).is_none());
}

#[test]
fn refresh_of_an_unknown_or_expired_session_gives_none() {
    let (auth, session, _dir) = signed_in();
    let hash = auth.session(&session.token, now()).expect("live");

    assert_eq!(auth.refresh("not a session", now()).expect("refresh"), None);
    assert_eq!(
        auth.refresh(&hash, session.expires_at).expect("refresh"),
        None
    );
}

#[test]
fn a_refreshed_expiry_survives_a_restart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Arc::new(redb::Database::create(dir.path().join("t.redb")).expect("db"));
    crate::db::init(&db).expect("init");
    let auth = AdminAuth::load(Arc::clone(&db), Duration::from_mins(15)).expect("load");
    auth.init(PASSWORD, now()).expect("init");
    let session = auth
        .login(CLIENT, ADMIN_USERNAME, PASSWORD, now())
        .expect("login");
    let hash = auth.session(&session.token, now()).expect("live");
    auth.refresh(&hash, now() + Duration::from_mins(10))
        .expect("refresh");
    drop(auth);

    let reloaded = AdminAuth::load(db, Duration::from_mins(15)).expect("reload");

    assert!(
        reloaded
            .session(&session.token, now() + Duration::from_mins(20))
            .is_some()
    );
}

#[test]
fn sessions_stored_before_refresh_existed_still_load() {
    let stored = serde_json::json!({ "expires_at": { "secs_since_epoch": 2_000_000_900u64, "nanos_since_epoch": 0 } });

    let record: SessionRecord = serde_json::from_value(stored).expect("old record");

    assert_eq!(record.started_at, None);
}
