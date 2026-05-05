//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Version information service

use anyhow::{anyhow, Result};
use crate::utils::{command_stdout, get_base_hw_part_number, get_current_side, get_next_side};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::LazyLock;

const SUMMIT_RCM_VERSION: &str = env!("CARGO_PKG_VERSION");

static RE_NMCLI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"nmcli.*version\s+(?P<VERSION>.*)").unwrap());
static RE_SUPPLICANT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"sdcsupp\s+v(?P<VERSION>.*)").unwrap());
static RE_OS_RELEASE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"VERSION="(?P<VERSION>.*)"#).unwrap());
static RE_BLUEZ: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"bluetoothctl: (?P<VERSION>.*)").unwrap());

pub struct VersionService;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub summit_rcm: String,
    pub nm_version: String,
    pub build: String,
    pub supplicant: String,
    pub radio_stack: String,
    pub driver: String,
    pub kernel_vermagic: String,
    pub bluez: String,
    pub u_boot: String,
    pub current_side: String,
    pub next_side: String,
    pub base_hw_part_number: String,
}

impl VersionService {
    pub async fn get_version_info() -> Result<VersionInfo> {
        Self::build_version_info().await
    }

    async fn build_version_info() -> Result<VersionInfo> {
        let nm_version = Self::get_nmcli_version().await?;
        let radio_stack = nm_version.split('-').next().unwrap_or("").to_string();
        let build = Self::get_os_release_version().await?;
        let supplicant = Self::get_supplicant_version().await?;
        let bluez = Self::get_bluez_version().await.unwrap_or_else(|| "n/a".to_string());
        let uboot = Self::get_uboot_version().await.unwrap_or_default();

        Ok(VersionInfo {
            summit_rcm: SUMMIT_RCM_VERSION.to_string(),
            nm_version,
            build,
            supplicant,
            radio_stack,
            driver: String::new(),
            kernel_vermagic: String::new(),
            bluez,
            u_boot: uboot,
            current_side: get_current_side().await.unwrap_or_else(|_| "unknown".to_string()),
            next_side: get_next_side().await.unwrap_or_else(|_| "unknown".to_string()),
            base_hw_part_number: get_base_hw_part_number().await.unwrap_or_default(),
        })
    }

    async fn get_nmcli_version() -> Result<String> {
        let out = command_stdout("nmcli", &["--version"]).await?;
        RE_NMCLI
            .captures(&out)
            .and_then(|captures| captures.name("VERSION"))
            .map(|value| value.as_str().to_string())
            .ok_or_else(|| anyhow!("Could not parse nmcli version"))
    }

    async fn get_supplicant_version() -> Result<String> {
        let out = command_stdout("sdcsupp", &["-v"]).await?;
        RE_SUPPLICANT
            .captures(&out)
            .and_then(|captures| captures.name("VERSION"))
            .map(|value| value.as_str().to_string())
            .ok_or_else(|| anyhow!("Could not parse supplicant version"))
    }

    async fn get_os_release_version() -> Result<String> {
        let content = tokio::fs::read_to_string("/etc/os-release").await?;
        Ok(RE_OS_RELEASE
            .captures(&content)
            .and_then(|c| c.name("VERSION"))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "Unknown".to_string()))
    }

    async fn get_uboot_version() -> Option<String> {
        command_stdout("fw_printenv", &["-n", "version"]).await.ok()
    }

    async fn get_bluez_version() -> Option<String> {
        let path = "/usr/bin/bluetoothctl";
        if !Path::new(path).exists() {
            return Some("Unknown".to_string());
        }
        let text = command_stdout(path, &["--version"]).await.ok()?;
        RE_BLUEZ.captures(&text)?.name("VERSION").map(|m| m.as_str().to_string())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/services/version_service.rs"]
mod tests;
