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
fn login_before_init_reports_not_initialized() {
    let (auth, _dir) = admin();

    assert!(matches!(
        auth.login(ADMIN_USERNAME, PASSWORD, now()),
        Err(AdminError::NotInitialized)
    ));
}

#[test]
fn login_issues_a_session_that_expires() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");

    let session = auth.login(ADMIN_USERNAME, PASSWORD, now()).expect("login");

    assert_eq!(session.token.len(), 64);
    assert!(auth.session(&session.token, now()).is_some());
    assert!(auth.session(&session.token, session.expires_at).is_none());
    assert!(auth.session("not-a-token", now()).is_none());
}

#[test]
fn wrong_user_and_wrong_password_fail_the_same_way() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");

    let wrong_user = auth.login("root", PASSWORD, now()).expect_err("wrong user");
    let wrong_password = auth
        .login(ADMIN_USERNAME, "wrong password!", now())
        .expect_err("wrong pw");

    assert_eq!(wrong_user.to_string(), wrong_password.to_string());
}

#[test]
fn five_failures_lock_the_account_for_fifteen_minutes() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    for _ in 0..MAX_FAILED_LOGINS {
        let _ = auth.login(ADMIN_USERNAME, "wrong password!", now());
    }

    let while_locked = auth.login(ADMIN_USERNAME, PASSWORD, now() + Duration::from_mins(14));
    let after_lock = auth.login(ADMIN_USERNAME, PASSWORD, now() + LOCKOUT_DURATION);

    assert!(matches!(while_locked, Err(AdminError::InvalidCredentials)));
    assert!(after_lock.is_ok());
}

#[test]
fn failures_outside_the_window_do_not_count() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    for i in 0..u64::try_from(MAX_FAILED_LOGINS).expect("small") {
        let at = now() + Duration::from_mins(16 * i);
        let _ = auth.login(ADMIN_USERNAME, "wrong password!", at);
    }

    let later = now() + Duration::from_mins(16 * 5);
    assert!(auth.login(ADMIN_USERNAME, PASSWORD, later).is_ok());
}

#[test]
fn logout_and_password_change_revoke_sessions() {
    let (auth, _dir) = admin();
    auth.init(PASSWORD, now()).expect("init");
    let first = auth.login(ADMIN_USERNAME, PASSWORD, now()).expect("login");
    let second = auth.login(ADMIN_USERNAME, PASSWORD, now()).expect("login");
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
    let session = auth.login(ADMIN_USERNAME, PASSWORD, now()).expect("login");
    drop(auth);

    let reloaded = AdminAuth::load(db, Duration::from_mins(15)).expect("reload");
    let live = reloaded.session(&session.token, now()).is_some();
    let purged = reloaded.purge_expired(session.expires_at).expect("purge");

    assert!(live);
    assert_eq!(purged, 1);
    assert!(reloaded.session(&session.token, now()).is_none());
}
