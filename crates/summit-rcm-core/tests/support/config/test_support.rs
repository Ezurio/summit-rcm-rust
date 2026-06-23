//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Shared configuration test seam.
//!
//! These helpers drive the in-memory configuration override store that
//! [`super::ServerConfig::get`] consults under test builds. They are exposed so
//! that downstream plugin crates can exercise configuration-dependent behavior
//! from their own colocated unit tests by enabling the `test-support` feature on
//! this crate as a dev-dependency.

use super::{SETTINGS_CONFIG, SummitRcmConfigManage, SystemSettingsManage};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

pub static SETTINGS_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
pub static SERVER_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

static SERVER_OVERRIDES: LazyLock<Mutex<HashMap<(String, String), String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static ENV_OVERRIDES: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn server_override(section: &str, key: &str) -> Option<String> {
    SERVER_OVERRIDES
        .lock()
        .unwrap()
        .get(&(section.to_string(), key.to_string()))
        .cloned()
}

pub fn set_server_override(section: &str, key: &str, value: &str) {
    SERVER_OVERRIDES
        .lock()
        .unwrap()
        .insert((section.to_string(), key.to_string()), value.to_string());
}

pub fn clear_server_overrides() {
    SERVER_OVERRIDES.lock().unwrap().clear();
}

pub fn env_override(key: &str) -> Option<String> {
    ENV_OVERRIDES.lock().unwrap().get(key).cloned()
}

pub fn set_env_override(key: &str, value: &str) {
    ENV_OVERRIDES
        .lock()
        .unwrap()
        .insert(key.to_string(), value.to_string());
}

pub fn clear_env_override(key: &str) {
    ENV_OVERRIDES.lock().unwrap().remove(key);
}

pub fn clear_env_overrides() {
    ENV_OVERRIDES.lock().unwrap().clear();
}

pub fn set_system_setting(key: &str, value: &str) -> bool {
    SystemSettingsManage::ensure_section();
    SummitRcmConfigManage::set(SystemSettingsManage::SECTION, key, value)
}

pub fn delete_system_setting(key: &str) {
    let mut cfg = SETTINGS_CONFIG.write().unwrap();
    cfg.remove_key(SystemSettingsManage::SECTION, key);
}

pub fn user_callback_timeout_secs() -> u64 {
    SystemSettingsManage::get_int("user_callback_timeout", 10) as u64
}
