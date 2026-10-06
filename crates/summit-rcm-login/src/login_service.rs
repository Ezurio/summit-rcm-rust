//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Session login management service

use rustix::time::Timespec;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use summit_rcm_core::config::ServerConfig;
use summit_rcm_core::config::SystemSettingsManage;
use summit_rcm_core::utils::{boottime, elapsed_timespec};

#[derive(Clone, Debug)]
struct TrackedSession {
    pub username: String,
    pub last_activity_at: Timespec,
}

struct LoginServiceState {
    failed_logins: HashMap<String, Vec<Timespec>>,
    tracked_sessions: HashMap<i128, TrackedSession>,
}

impl LoginServiceState {
    fn track_session(
        &mut self,
        session_id: i128,
        username: &str,
        last_activity_at: Timespec,
        timeout: Duration,
    ) {
        self.tracked_sessions.retain(|_, session| {
            elapsed_timespec(last_activity_at, session.last_activity_at) < timeout
        });
        let _ = self.tracked_sessions.insert(
            session_id,
            TrackedSession {
                username: username.to_string(),
                last_activity_at,
            },
        );
    }

    // O(1) check+refresh: no full-scan prune, evicts the one expired session if found.
    fn check_and_refresh(&mut self, session_id: i128, now: Timespec, timeout: Duration) -> bool {
        let Some(session) = self.tracked_sessions.get_mut(&session_id) else {
            return false;
        };
        if elapsed_timespec(now, session.last_activity_at) < timeout {
            session.last_activity_at = now;
            true
        } else {
            let _ = self.tracked_sessions.remove(&session_id);
            false
        }
    }
}

static LOGIN_STATE: LazyLock<Mutex<LoginServiceState>> = LazyLock::new(|| {
    Mutex::new(LoginServiceState {
        failed_logins: HashMap::new(),
        tracked_sessions: HashMap::new(),
    })
});

fn current_boottime() -> Timespec {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(override_now) = test_support::boottime_override() {
        return override_now;
    }

    boottime()
}

pub(crate) struct LoginService;

impl LoginService {
    fn session_timeout() -> Duration {
        summit_rcm_core::cached_config!(
            Duration,
            Duration::from_secs(SystemSettingsManage::get_int("session_timeout", 10) as u64 * 60)
        )
    }

    pub(crate) fn default_username() -> String {
        ServerConfig::get_string("summit-rcm", "default_username", "root")
    }

    pub(crate) fn default_password() -> String {
        ServerConfig::get_string("summit-rcm", "default_password", "summit")
    }

    pub(crate) fn allow_multiple_user_sessions() -> bool {
        summit_rcm_core::cached_config!(
            bool,
            ServerConfig::get_bool("summit-rcm", "allow_multiple_user_sessions", false)
        )
    }

    /// Return true if the user is blocked due to too many failed login attempts
    pub(crate) fn is_user_blocked(username: &str) -> bool {
        let (retry_times_raw, tamper_timeout_raw) = SystemSettingsManage::get_two_ints(
            "login_retry_times",
            5,
            "tamper_protection_timeout",
            600,
        );
        let retry_times = retry_times_raw as usize;
        let tamper_timeout = Duration::from_secs(tamper_timeout_raw as u64);
        let mut state = LOGIN_STATE.lock().unwrap();
        let now = current_boottime();
        if let Some(times) = state.failed_logins.get_mut(username)
            && times.len() >= retry_times
        {
            let dt = elapsed_timespec(now, *times.last().unwrap_or(&now));
            if dt < tamper_timeout {
                return true;
            }
            let _ = state.failed_logins.remove(username);
        }
        false
    }

    /// Record a failed login attempt
    pub(crate) fn login_failed(username: &str) {
        let window =
            Duration::from_secs(SystemSettingsManage::get_int("login_retry_window", 600) as u64);
        let mut state = LOGIN_STATE.lock().unwrap();
        let now = current_boottime();
        let times = state.failed_logins.entry(username.to_string()).or_default();
        times.retain(|&t| elapsed_timespec(now, t) < window);
        times.push(now);
    }

    /// Reset failed login counter on successful login
    pub(crate) fn login_reset(username: &str) {
        let _ = LOGIN_STATE.lock().unwrap().failed_logins.remove(username);
    }

    /// Return true if the user already has an active session
    pub(crate) fn is_user_logged_in(username: &str) -> bool {
        let now = current_boottime();
        let timeout = Self::session_timeout();
        let state = LOGIN_STATE.lock().unwrap();
        state.tracked_sessions.values().any(|session| {
            session.username == username
                && elapsed_timespec(now, session.last_activity_at) < timeout
        })
    }

    #[cfg(feature = "api-legacy")]
    pub(crate) fn is_session_active(session_id: i128) -> bool {
        let now = current_boottime();
        let timeout = Self::session_timeout();
        let state = LOGIN_STATE.lock().unwrap();
        state
            .tracked_sessions
            .get(&session_id)
            .map(|session| elapsed_timespec(now, session.last_activity_at) < timeout)
            .unwrap_or(false)
    }

    /// Check session validity and refresh its timestamp in a single lock acquisition.
    /// Returns false if the session is missing or expired.
    pub(crate) fn check_and_refresh_session(session_id: i128) -> bool {
        let now = current_boottime();
        let timeout = Self::session_timeout();
        LOGIN_STATE
            .lock()
            .unwrap()
            .check_and_refresh(session_id, now, timeout)
    }

    pub(crate) fn track_session(session_id: i128, username: &str) {
        let now = current_boottime();
        let timeout = Self::session_timeout();
        LOGIN_STATE
            .lock()
            .unwrap()
            .track_session(session_id, username, now, timeout);
    }

    pub(crate) fn remove_session(session_id: i128) {
        let _ = LOGIN_STATE
            .lock()
            .unwrap()
            .tracked_sessions
            .remove(&session_id);
    }
}

#[cfg(any(test, feature = "test-support"))]
#[path = "../tests/login_service/test_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../tests/login_service/tests.rs"]
mod tests;
