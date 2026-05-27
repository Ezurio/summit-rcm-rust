use super::{BOOTTIME_OVERRIDE_SECS, LOGIN_STATE};
use std::sync::atomic::Ordering;

pub fn clear_test_state() {
    let mut state = LOGIN_STATE.lock().unwrap();
    state.failed_logins.clear();
    state.tracked_sessions.clear();
    BOOTTIME_OVERRIDE_SECS.store(u64::MAX, Ordering::Relaxed);
}

pub fn clear_login_state() {
    let mut state = LOGIN_STATE.lock().unwrap();
    state.failed_logins.clear();
    state.tracked_sessions.clear();
}

pub fn set_boottime_secs(value: u64) {
    BOOTTIME_OVERRIDE_SECS.store(value, Ordering::Relaxed);
}
