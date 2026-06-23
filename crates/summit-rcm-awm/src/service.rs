//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! AWM (Adaptive Wi-Fi Management) configuration service

use anyhow::{bail, Context, Result};
use summit_rcm_core::utils::{path_exists, read_text};
use configparser::ini::Ini;
use std::path::Path;

const ADAPTIVE_WW_CONFIG_FILE: &str = "/etc/default/adaptive_ww";
const SUMMIT_RCM_AWM_PLUGIN_INI_FILE: &str = "/etc/summit-rcm-awm.ini";

pub struct AwmConfigService;

impl AwmConfigService {
    async fn load_ini(path: &str) -> Result<Ini> {
        let mut ini = Ini::new();
        let _ = ini.load_async(path)
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        Ok(ini)
    }

    async fn get_awm_cfg() -> Result<String> {
        let ini = Self::load_ini(SUMMIT_RCM_AWM_PLUGIN_INI_FILE)
            .await
            .context("AWM plugin INI not found")?;
        if let Some(path) = ini.get("summit-rcm", "awm_cfg") {
            let path = path.trim_matches('"').to_string();
            if !path.is_empty() && path_exists(&path).await {
                return Ok(path);
            }
        }
        bail!("awm_cfg not found or file does not exist")
    }

    async fn read_awm_file(path: &str) -> Ini {
        Self::load_ini(path).await.unwrap_or_default()
    }

    async fn write_awm_file(path: &str, ini: &Ini) -> Result<()> {
        if let Some(parent) = Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        ini.write_async(path)
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        Ok(())
    }

    pub async fn get_scan_attempts() -> Result<i32> {
        let path = Self::get_awm_cfg().await?;
        let ini = Self::read_awm_file(&path).await;
        let val = ini
            .get("default", "scan_attempts")
            .context("scan_attempts not found")?;
        val.parse::<i32>().context("invalid scan_attempts value")
    }

    pub async fn set_scan_attempts(enable: i32) -> Result<()> {
        let path = Self::get_awm_cfg().await?;
        let mut ini = Self::read_awm_file(&path).await;
        if enable != 0 {
            let _ = ini.remove_key("default", "scan_attempts");
        } else {
            let _ = ini.set("default", "scan_attempts", Some("0".to_string()));
        }
        Self::write_awm_file(&path, &ini).await
    }

    pub async fn get_lite_mode_enabled() -> bool {
        read_text(ADAPTIVE_WW_CONFIG_FILE)
            .await
            .map(|c| c.to_lowercase().contains("lite"))
            .unwrap_or(false)
    }
}
