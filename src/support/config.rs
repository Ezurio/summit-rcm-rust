//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! INI-based configuration management (port of Python SummitRCMConfigManage / SystemSettingsManage)

use anyhow::Context;
use configparser::ini::Ini;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use log::warn;

pub const SUMMIT_RCM_SERVER_CONF_FILE: &str = "/etc/summit-rcm.ini";
pub const SUMMIT_RCM_SETTINGS_FILE: &str = "/etc/summit-rcm/summit-rcm-settings.ini";

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
    cfg.sections().iter().any(|existing| existing == section)
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

static SETTINGS_CONFIG: LazyLock<Mutex<Ini>> = LazyLock::new(|| {
    let mut ini = Ini::new();
    if Path::new(SETTINGS_CONFIG_PATH.as_str()).exists()
        && let Err(e) = ini.load(SETTINGS_CONFIG_PATH.as_str())
    {
        warn!("Could not load settings file {}: {}", SETTINGS_CONFIG_PATH.as_str(), e);
    }
    Mutex::new(ini)
});

pub struct SummitRcmConfigManage;

impl SummitRcmConfigManage {
    pub fn has_section(section: &str) -> bool {
        let cfg = SETTINGS_CONFIG.lock().unwrap();
        has_section(&cfg, section)
    }

    pub fn add_section(section: &str) -> bool {
        let mut cfg = SETTINGS_CONFIG.lock().unwrap();
        if !has_section(&cfg, section) {
            cfg.set(section, "__init__", None); // ensure section exists
            true
        } else {
            false
        }
    }

    pub fn remove_section(section: &str) -> bool {
        let mut cfg = SETTINGS_CONFIG.lock().unwrap();
        if has_section(&cfg, section) {
            cfg.remove_section(section);
            true
        } else {
            false
        }
    }

    pub fn set(section: &str, key: &str, value: &str) -> bool {
        let mut cfg = SETTINGS_CONFIG.lock().unwrap();
        if has_section(&cfg, section) {
            cfg.set(section, key, Some(value.to_string()));
            true
        } else {
            false
        }
    }

    pub fn get(section: &str, key: &str) -> Option<String> {
        SETTINGS_CONFIG.lock().unwrap().get(section, key)
    }

    #[cfg(feature = "unauthenticated")]
    pub fn get_bool(section: &str, key: &str, fallback: bool) -> bool {
        Self::get(section, key)
            .and_then(|value| parse_bool(&value))
            .unwrap_or(fallback)
    }

    pub fn get_string(section: &str, key: &str, default: &str) -> String {
        Self::get(section, key)
            .unwrap_or_else(|| default.to_owned())
            .trim_matches('"')
            .to_string()
    }

    pub fn sections_with_key(key: &str) -> Vec<String> {
        let cfg = SETTINGS_CONFIG.lock().unwrap();
        cfg.sections()
            .into_iter()
            .filter(|s| cfg.get(s, key).is_some())
            .collect()
    }

    pub fn sections_and_key(key: &str) -> HashMap<String, String> {
        let cfg = SETTINGS_CONFIG.lock().unwrap();
        cfg.sections()
            .into_iter()
            .filter_map(|s| cfg.get(&s, key).map(|v| (s, v)))
            .collect()
    }

    pub fn save() -> anyhow::Result<()> {
        let cfg = SETTINGS_CONFIG.lock().unwrap();
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
    pub(crate) const SECTION: &'static str = "settings";

    pub fn ensure_section() {
        SummitRcmConfigManage::add_section(Self::SECTION);
    }

    pub fn get_int(key: &str, fallback: i64) -> i64 {
        SummitRcmConfigManage::get(Self::SECTION, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
    }
}

// ---------------------------------------------------------------------------
// ServerConfig – reads the main summit-rcm.ini (server / startup config)
// ---------------------------------------------------------------------------

static SERVER_CONFIG: LazyLock<Ini> = LazyLock::new(|| {
    let mut ini = Ini::new();
    if let Err(e) = ini.load(SERVER_CONFIG_PATH.as_str()) {
        warn!("Could not load server config {}: {}", SERVER_CONFIG_PATH.as_str(), e);
    }
    ini
});

pub struct ServerConfig;

impl ServerConfig {
    pub fn get(section: &str, key: &str) -> Option<String> {
        #[cfg(test)]
        if let Some(value) = tests::server_override(section, key) {
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
        Self::get(section, key)
            .unwrap_or_else(|| default.to_owned())
            .trim_matches('"')
            .to_string()
    }

    #[cfg(any(feature = "at-interface", test))]
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

#[cfg(test)]
#[path = "../../tests/unit/config.rs"]
pub(crate) mod tests;
