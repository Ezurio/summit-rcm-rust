//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! AWM (Adaptive Wi-Fi Management) configuration service

use anyhow::{bail, Context, Result};
use configparser::ini::Ini;
use std::path::Path;

const ADAPTIVE_WW_CONFIG_FILE: &str = "/etc/default/adaptive_ww";
const SUMMIT_RCM_AWM_PLUGIN_INI_FILE: &str = "/etc/summit-rcm-awm.ini";

pub struct AwmConfigService;

impl AwmConfigService {
    fn load_ini(path: &str) -> Result<Ini> {
        let mut ini = Ini::new();
        ini.load(path)
            .map_err(|error| anyhow::anyhow!(error))?;
        Ok(ini)
    }

    fn get_awm_cfg() -> Result<String> {
        let ini = Self::load_ini(SUMMIT_RCM_AWM_PLUGIN_INI_FILE)
            .context("AWM plugin INI not found")?;
        if let Some(path) = ini.get("summit-rcm", "awm_cfg") {
            let path = path.trim_matches('"').to_string();
            if !path.is_empty() && Path::new(&path).exists() {
                return Ok(path);
            }
        }
        bail!("awm_cfg not found or file does not exist")
    }

    fn read_awm_file(path: &str) -> Ini {
        Self::load_ini(path).unwrap_or_default()
    }

    fn write_awm_file(path: &str, ini: &Ini) -> Result<()> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        ini.write(path)
            .map_err(|error| anyhow::anyhow!(error))?;
        Ok(())
    }

    pub fn get_scan_attempts() -> Result<i32> {
        let path = Self::get_awm_cfg()?;
        let ini = Self::read_awm_file(&path);
        let val = ini
            .get("default", "scan_attempts")
            .context("scan_attempts not found")?;
        val.parse::<i32>().context("invalid scan_attempts value")
    }

    pub fn set_scan_attempts(enable: i32) -> Result<()> {
        let path = Self::get_awm_cfg()?;
        let mut ini = Self::read_awm_file(&path);
        if enable != 0 {
            ini.remove_key("default", "scan_attempts");
        } else {
            ini.set("default", "scan_attempts", Some("0".to_string()));
        }
        Self::write_awm_file(&path, &ini)
    }

    pub fn get_lite_mode_enabled() -> bool {
        std::fs::read_to_string(ADAPTIVE_WW_CONFIG_FILE)
            .map(|c| c.to_lowercase().contains("lite"))
            .unwrap_or(false)
    }
}
