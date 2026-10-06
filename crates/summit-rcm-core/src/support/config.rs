//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! INI-based configuration management (port of Python SummitRCMConfigManage / SystemSettingsManage)

use crate::utils::path_exists_sync;
use anyhow::Context;
use configparser::ini::Ini;
use log::warn;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

pub const SUMMIT_RCM_SERVER_CONF_FILE: &str = "/etc/summit-rcm.ini";
pub const SUMMIT_RCM_SETTINGS_FILE: &str = "/etc/summit-rcm/summit-rcm-settings.ini";

/// Returns the env var value (trimmed of surrounding `"`) in non-release
/// builds; always returns `default` unchanged in release builds.
pub fn env_or_trimmed(key: &str, default: &str) -> String {
    #[cfg(not(debug_assertions))]
    let _ = key;

    #[cfg(any(test, feature = "test-support"))]
    if let Some(val) = test_support::env_override(key) {
        return val.trim_matches('"').to_string();
    }

    #[cfg(debug_assertions)]
    if let Ok(val) = std::env::var(key) {
        return val.trim_matches('"').to_string();
    }
    default.to_string()
}

fn parse_bool(value: &str) -> Option<bool> {
    if matches!(value, "1")
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
    {
        return Some(true);
    }

    if matches!(value, "0")
        || value.eq_ignore_ascii_case("false")
        || value.eq_ignore_ascii_case("no")
        || value.eq_ignore_ascii_case("off")
    {
        return Some(false);
    }

    None
}

fn has_section(cfg: &Ini, section: &str) -> bool {
    cfg.get_map_ref().contains_key(section)
}

static SETTINGS_CONFIG_PATH: LazyLock<String> = LazyLock::new(|| {
    std::env::var("SUMMIT_RCM_SETTINGS_FILE")
        .unwrap_or_else(|_| SUMMIT_RCM_SETTINGS_FILE.to_string())
});

static SERVER_CONFIG_PATH: LazyLock<String> = LazyLock::new(|| {
    std::env::var("SUMMIT_RCM_SERVER_CONF_FILE")
        .unwrap_or_else(|_| SUMMIT_RCM_SERVER_CONF_FILE.to_string())
});

// ---------------------------------------------------------------------------
// SummitRCMConfigManage – persisted settings (summit-rcm-settings.ini)
// ---------------------------------------------------------------------------

static SETTINGS_CONFIG: LazyLock<RwLock<Ini>> = LazyLock::new(|| {
    let mut ini = Ini::new();
    if path_exists_sync(SETTINGS_CONFIG_PATH.as_str())
        && let Err(e) = ini.load(SETTINGS_CONFIG_PATH.as_str())
    {
        warn!(
            "Could not load settings file {}: {}",
            SETTINGS_CONFIG_PATH.as_str(),
            e
        );
    }
    RwLock::new(ini)
});

pub struct SummitRcmConfigManage;

impl SummitRcmConfigManage {
    pub fn has_section(section: &str) -> bool {
        let cfg = SETTINGS_CONFIG.read().unwrap();
        has_section(&cfg, section)
    }

    pub fn add_section(section: &str) -> bool {
        let mut cfg = SETTINGS_CONFIG.write().unwrap();
        if !has_section(&cfg, section) {
            cfg.set(section, "__init__", None); // ensure section exists
            true
        } else {
            false
        }
    }

    pub fn remove_section(section: &str) -> bool {
        let mut cfg = SETTINGS_CONFIG.write().unwrap();
        if has_section(&cfg, section) {
            cfg.remove_section(section);
            true
        } else {
            false
        }
    }

    pub fn set(section: &str, key: &str, value: &str) -> bool {
        let mut cfg = SETTINGS_CONFIG.write().unwrap();
        if has_section(&cfg, section) {
            cfg.set(section, key, Some(value.to_string()));
            true
        } else {
            false
        }
    }

    pub fn get(section: &str, key: &str) -> Option<String> {
        SETTINGS_CONFIG.read().unwrap().get(section, key)
    }

    pub fn get_bool(section: &str, key: &str, fallback: bool) -> bool {
        Self::get(section, key)
            .and_then(|value| parse_bool(&value))
            .unwrap_or(fallback)
    }

    pub fn get_string(section: &str, key: &str, default: &str) -> String {
        let mut s = Self::get(section, key).unwrap_or_else(|| default.to_owned());
        if s.starts_with('"') || s.ends_with('"') {
            s = s.trim_matches('"').to_string();
        }
        s
    }

    pub fn sections_with_key(key: &str) -> Vec<String> {
        let cfg = SETTINGS_CONFIG.read().unwrap();
        cfg.get_map_ref()
            .iter()
            .filter(|(_, map)| map.contains_key(key))
            .map(|(section, _)| section.clone())
            .collect()
    }

    pub fn sections_and_key(key: &str) -> HashMap<String, String> {
        let cfg = SETTINGS_CONFIG.read().unwrap();
        cfg.get_map_ref()
            .iter()
            .filter_map(|(section, map)| {
                map.get(key)
                    .and_then(Option::as_deref)
                    .map(|v| (section.clone(), v.to_owned()))
            })
            .collect()
    }

    pub fn count_sections_with_key(key: &str) -> usize {
        let cfg = SETTINGS_CONFIG.read().unwrap();
        cfg.get_map_ref()
            .values()
            .filter(|map| map.contains_key(key))
            .count()
    }

    /// Fetch two keys from the same section under a single read lock.
    pub fn get_two(section: &str, key1: &str, key2: &str) -> Option<(String, String)> {
        let cfg = SETTINGS_CONFIG.read().unwrap();
        let map = cfg.get_map_ref().get(section)?;
        let v1 = map.get(key1)?.as_deref()?.to_owned();
        let v2 = map.get(key2)?.as_deref()?.to_owned();
        Some((v1, v2))
    }

    /// Set a key only if it already exists, under a single write lock.
    pub fn set_if_key_exists(section: &str, key: &str, value: &str) -> bool {
        let mut cfg = SETTINGS_CONFIG.write().unwrap();
        let exists = cfg
            .get_map_ref()
            .get(section)
            .is_some_and(|m| m.contains_key(key));
        if exists {
            cfg.set(section, key, Some(value.to_string()));
        }
        exists
    }

    /// Write multiple key/value pairs to a section under a single write lock.
    pub fn set_many(section: &str, entries: &[(&str, &str)]) -> bool {
        let mut cfg = SETTINGS_CONFIG.write().unwrap();
        if !has_section(&cfg, section) {
            return false;
        }
        for &(key, value) in entries {
            cfg.set(section, key, Some(value.to_string()));
        }
        true
    }

    pub fn save() -> anyhow::Result<()> {
        let cfg = SETTINGS_CONFIG.read().unwrap();
        cfg.write(SETTINGS_CONFIG_PATH.as_str())
            .context("Failed to write settings file")?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SystemSettingsManage – helpers for the [settings] section
// ---------------------------------------------------------------------------

pub struct SystemSettingsManage;

impl SystemSettingsManage {
    pub const SECTION: &'static str = "settings";

    pub fn ensure_section() {
        SummitRcmConfigManage::add_section(Self::SECTION);
    }

    pub fn get_int(key: &str, fallback: i64) -> i64 {
        SummitRcmConfigManage::get(Self::SECTION, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
    }

    /// Fetch two integer settings from [settings] under a single read lock.
    pub fn get_two_ints(key1: &str, default1: i64, key2: &str, default2: i64) -> (i64, i64) {
        let cfg = SETTINGS_CONFIG.read().unwrap();
        let section_map = cfg.get_map_ref().get(Self::SECTION);
        let v1 = section_map
            .and_then(|m| m.get(key1))
            .and_then(Option::as_deref)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default1);
        let v2 = section_map
            .and_then(|m| m.get(key2))
            .and_then(Option::as_deref)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default2);
        (v1, v2)
    }
}

// ---------------------------------------------------------------------------
// cached_config! – cache a config value once at first use in production;
// read it live on every call in tests so per-test overrides take effect.
//
// Usage:
//   crate::cached_config!(bool,   ServerConfig::get_bool("s", "k", default))
//   crate::cached_config!(String, ServerConfig::get_string("s", "k", default))
// ---------------------------------------------------------------------------

/// Whether configuration caching is disabled by *this crate's* build
/// configuration. Evaluated in core's compilation context so the `test-support`
/// feature is honored even when `cached_config!` is expanded from a caller crate
/// that does not define that feature. Caller-side `#[cfg(test)]` is handled
/// separately inside the macro via `cfg!(test)`.
pub const CONFIG_CACHING_DISABLED: bool = cfg!(any(test, feature = "test-support"));

#[macro_export]
macro_rules! cached_config {
    ($T:ty, $expr:expr) => {{
        // `cfg!(test)` expands in the caller crate, so any crate's own unit-test
        // build reads live configuration. `CONFIG_CACHING_DISABLED` additionally
        // disables caching for integration tests that enable core's
        // `test-support` feature.
        if cfg!(test) || $crate::config::CONFIG_CACHING_DISABLED {
            $expr
        } else {
            static V: std::sync::LazyLock<$T> = std::sync::LazyLock::new(|| $expr);
            ::std::ops::Deref::deref(&V).clone()
        }
    }};
}

// ---------------------------------------------------------------------------
// ServerConfig – reads the main summit-rcm.ini (server / startup config)
// ---------------------------------------------------------------------------

static SERVER_CONFIG: LazyLock<Ini> = LazyLock::new(|| {
    let mut ini = Ini::new();
    if let Err(e) = ini.load(SERVER_CONFIG_PATH.as_str()) {
        warn!(
            "Could not load server config {}: {}",
            SERVER_CONFIG_PATH.as_str(),
            e
        );
    }
    ini
});

pub struct ServerConfig;

impl ServerConfig {
    pub fn get(section: &str, key: &str) -> Option<String> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(value) = test_support::server_override(section, key) {
            return Some(value);
        }

        SERVER_CONFIG.get(section, key)
    }

    pub fn get_bool(section: &str, key: &str, fallback: bool) -> bool {
        Self::get(section, key)
            .and_then(|value| parse_bool(&value))
            .unwrap_or(fallback)
    }

    pub fn get_string(section: &str, key: &str, default: &str) -> String {
        let mut s = Self::get(section, key).unwrap_or_else(|| default.to_owned());
        if s.starts_with('"') || s.ends_with('"') {
            s = s.trim_matches('"').to_string();
        }
        s
    }

    pub fn get_u32(section: &str, key: &str, fallback: u32) -> u32 {
        Self::get(section, key)
            .and_then(|value| value.trim().parse::<u32>().ok())
            .unwrap_or(fallback)
    }

    pub fn get_words(section: &str, key: &str) -> Vec<String> {
        Self::get(section, key)
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    pub fn section_keys(section: &str) -> Vec<String> {
        if let Some(section_map) = SERVER_CONFIG.get_map_ref().get(section) {
            let mut keys: Vec<String> = section_map.keys().cloned().collect();
            keys.sort();
            keys
        } else {
            Vec::new()
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
#[path = "../../tests/support/config/test_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "../../tests/support/config/tests.rs"]
mod tests;
