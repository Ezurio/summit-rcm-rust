use super::{test_support, LoginService};
use crate::config::tests;

const LOGIN_TEST_KEYS: &[&str] = &[
    "login_retry_times",
    "login_retry_window",
    "tamper_protection_timeout",
];

fn clear_login_settings() {
    for key in LOGIN_TEST_KEYS {
        tests::delete_system_setting(key);
    }
}

struct TestCleanup;

impl Drop for TestCleanup {
    fn drop(&mut self) {
        test_support::clear_test_state();
        clear_login_settings();
    }
}

#[test]
fn failed_logins_block_within_retry_window_and_tamper_timeout() {
    let _guard = tests::SETTINGS_LOCK.lock();
    let _cleanup = TestCleanup;
    test_support::clear_login_state();
    clear_login_settings();

    assert!(tests::set_system_setting("login_retry_times", "3"));
    assert!(tests::set_system_setting("login_retry_window", "10"));
    assert!(tests::set_system_setting("tamper_protection_timeout", "30"));

    test_support::set_boottime_secs(100);
    LoginService::login_failed("root");
    test_support::set_boottime_secs(103);
    LoginService::login_failed("root");
    test_support::set_boottime_secs(108);
    LoginService::login_failed("root");

    assert!(LoginService::is_user_blocked("root"));

    test_support::set_boottime_secs(139);
    assert!(!LoginService::is_user_blocked("root"));
}

#[test]
fn failed_logins_outside_retry_window_do_not_accumulate() {
    let _guard = tests::SETTINGS_LOCK.lock();
    let _cleanup = TestCleanup;
    test_support::clear_login_state();
    clear_login_settings();

    assert!(tests::set_system_setting("login_retry_times", "2"));
    assert!(tests::set_system_setting("login_retry_window", "5"));
    assert!(tests::set_system_setting("tamper_protection_timeout", "20"));

    test_support::set_boottime_secs(100);
    LoginService::login_failed("root");
    test_support::set_boottime_secs(106);
    LoginService::login_failed("root");

    assert!(!LoginService::is_user_blocked("root"));
}

#[test]
fn tracked_sessions_expire_after_timeout() {
    let _guard = tests::SETTINGS_LOCK.lock();
    let _cleanup = TestCleanup;
    test_support::clear_login_state();

    test_support::set_boottime_secs(500);
    LoginService::track_session("session-a", "root", 10);
    assert!(LoginService::is_user_logged_in("root"));

    test_support::set_boottime_secs(511);
    assert!(!LoginService::is_user_logged_in("root"));
}