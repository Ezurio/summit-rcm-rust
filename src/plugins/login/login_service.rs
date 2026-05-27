//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Session login management service

use crate::config::SystemSettingsManage;
use crate::config::ServerConfig;
use crate::utils::{boottime, elapsed_timespec};
use rustix::time::Timespec;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
struct TrackedSession {
    pub username: String,
    pub started_at: Timespec,
    pub timeout: Duration,
}

struct LoginServiceState {
    failed_logins: HashMap<String, Vec<Timespec>>,
    tracked_sessions: HashMap<String, TrackedSession>,
}

impl LoginServiceState {
    fn prune_expired_sessions(&mut self, now: Timespec) {
        self.tracked_sessions
            .retain(|_, session| elapsed_timespec(now, session.started_at) < session.timeout);
    }

    fn track_session(
        &mut self,
        session_id: &str,
        username: &str,
        started_at: Timespec,
        timeout: Duration,
    ) {
        self.tracked_sessions.insert(
            session_id.to_string(),
            TrackedSession {
                username: username.to_string(),
                started_at,
                timeout,
            },
        );
    }
}

static LOGIN_STATE: LazyLock<Mutex<LoginServiceState>> = LazyLock::new(|| {
    Mutex::new(LoginServiceState {
        failed_logins: HashMap::new(),
        tracked_sessions: HashMap::new(),
    })
});

static BOOTTIME_OVERRIDE_SECS: AtomicU64 = AtomicU64::new(u64::MAX);

fn current_boottime() -> Timespec {
    let override_now = BOOTTIME_OVERRIDE_SECS.load(Ordering::Relaxed);
    if override_now != u64::MAX {
        return Timespec {
            tv_sec: override_now.try_into().unwrap_or(i64::MAX),
            tv_nsec: 0,
        };
    }

    boottime()
}

pub struct LoginService;

impl LoginService {
    pub fn default_username() -> String {
        ServerConfig::get_string("summit-rcm", "default_username", "root")
    }

    pub fn default_password() -> String {
        ServerConfig::get_string("summit-rcm", "default_password", "summit")
    }

    pub fn allow_multiple_user_sessions() -> bool {
        ServerConfig::get_bool("summit-rcm", "allow_multiple_user_sessions", false)
    }

    /// Return true if the user is blocked due to too many failed login attempts
    pub fn is_user_blocked(username: &str) -> bool {
        let mut state = LOGIN_STATE.lock().unwrap();
        let now = current_boottime();
        if let Some(times) = state.failed_logins.get_mut(username) {
            let retry_times = SystemSettingsManage::get_int("login_retry_times", 5) as usize;
            if times.len() >= retry_times {
                let dt = elapsed_timespec(now, *times.last().unwrap_or(&now));
                if dt < Duration::from_secs(SystemSettingsManage::get_int("tamper_protection_timeout", 600) as u64) {
                    return true;
                }
                state.failed_logins.remove(username);
            }
        }
        false
    }

    /// Record a failed login attempt
    pub fn login_failed(username: &str) {
        let mut state = LOGIN_STATE.lock().unwrap();
        let now = current_boottime();
        let window = Duration::from_secs(SystemSettingsManage::get_int("login_retry_window", 600) as u64);
        let times = state.failed_logins.entry(username.to_string()).or_default();
        times.retain(|&t| elapsed_timespec(now, t) < window);
        times.push(now);
    }

    /// Reset failed login counter on successful login
    pub fn login_reset(username: &str) {
        LOGIN_STATE.lock().unwrap().failed_logins.remove(username);
    }

    /// Return true if the user already has an active session
    pub fn is_user_logged_in(username: &str) -> bool {
        let now = current_boottime();
        let mut state = LOGIN_STATE.lock().unwrap();
        state.prune_expired_sessions(now);
        state
            .tracked_sessions
            .values()
            .any(|session| session.username == username)
    }

    pub fn is_session_active(session_id: &str) -> bool {
        let now = current_boottime();
        let mut state = LOGIN_STATE.lock().unwrap();
        state.prune_expired_sessions(now);
        state.tracked_sessions.contains_key(session_id)
    }

    pub fn track_session(session_id: &str, username: &str, timeout_secs: u64) {
        let now = current_boottime();
        LOGIN_STATE.lock().unwrap().track_session(
            session_id,
            username,
            now,
            Duration::from_secs(timeout_secs),
        );
    }

    pub fn remove_session(session_id: &str) {
        LOGIN_STATE.lock().unwrap().tracked_sessions.remove(session_id);
    }

}

#[cfg(test)]
#[path = "../../../tests/unit/services/login_service_test_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../../tests/unit/services/login_service.rs"]
mod tests;
