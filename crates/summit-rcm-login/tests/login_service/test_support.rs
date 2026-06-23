use super::LOGIN_STATE;
use rustix::time::Timespec;
use std::sync::atomic::{AtomicU64, Ordering};

static BOOTTIME_OVERRIDE_SECS: AtomicU64 = AtomicU64::new(u64::MAX);
static BOOTTIME_OVERRIDE_NSECS: AtomicU64 = AtomicU64::new(0);

pub fn boottime_override() -> Option<Timespec> {
    let secs = BOOTTIME_OVERRIDE_SECS.load(Ordering::Relaxed);
    if secs == u64::MAX {
        return None;
    }

    Some(Timespec {
        tv_sec: secs.try_into().unwrap_or(i64::MAX),
        tv_nsec: BOOTTIME_OVERRIDE_NSECS
            .load(Ordering::Relaxed)
            .try_into()
            .unwrap_or(i64::MAX),
    })
}

pub fn clear_test_state() {
    let mut state = LOGIN_STATE.lock().unwrap();
    state.failed_logins.clear();
    state.tracked_sessions.clear();
    BOOTTIME_OVERRIDE_SECS.store(u64::MAX, Ordering::Relaxed);
    BOOTTIME_OVERRIDE_NSECS.store(0, Ordering::Relaxed);
}

pub fn clear_login_state() {
    let mut state = LOGIN_STATE.lock().unwrap();
    state.failed_logins.clear();
    state.tracked_sessions.clear();
}

pub fn set_boottime_secs(value: u64) {
    BOOTTIME_OVERRIDE_SECS.store(value, Ordering::Relaxed);
    BOOTTIME_OVERRIDE_NSECS.store(0, Ordering::Relaxed);
}

pub fn set_boottime(value_secs: u64, value_nsecs: i64) {
    BOOTTIME_OVERRIDE_SECS.store(value_secs, Ordering::Relaxed);
    BOOTTIME_OVERRIDE_NSECS.store(value_nsecs.try_into().unwrap_or(u64::MAX), Ordering::Relaxed);
}
