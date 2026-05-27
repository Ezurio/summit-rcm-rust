//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Version information service

use anyhow::{anyhow, Result};
use crate::utils::{command_stdout, get_boot_rootfs_info, get_boot_rootfs_next_side};
use serde::{Deserialize, Serialize};
use std::{future::Future, path::Path, sync::LazyLock};
use tokio::sync::OnceCell;

const SUMMIT_RCM_VERSION: &str = env!("SUMMIT_RCM_BUILD_VERSION");

type CachedResult<T> = std::result::Result<T, String>;

static VERSION_INFO_CACHE: LazyLock<OnceCell<CachedResult<VersionInfo>>> =
    LazyLock::new(OnceCell::new);

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
        let mut version_info = get_cached_result(&VERSION_INFO_CACHE, Self::build_cached_version_info).await?;
        if version_info.current_side == "sd" {
            version_info.next_side = "sd".to_string();
            return Ok(version_info);
        }

        version_info.next_side = get_boot_rootfs_next_side()
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        Ok(version_info)
    }

    async fn build_cached_version_info() -> Result<VersionInfo> {
        let nm_version = get_network_manager_version().await?;
        let driver = get_wireless_driver().await?.unwrap_or_default();
        let radio_stack = nm_version.split('-').next().unwrap_or("").to_string();
        let kernel_vermagic = get_kernel_vermagic().await?;
        let build = get_os_release_info().await?;
        let supplicant = get_supplicant_version().await?;
        let bluez = get_bluez_version().await.unwrap_or_else(|| "n/a".to_string());
        let uboot = Self::get_uboot_version().await.unwrap_or_default();
        let (current_side, next_side, base_hw_part_number) = match get_boot_rootfs_info().await {
            Ok(info) if info.is_running_on_sd() => (
                "sd".to_string(),
                "sd".to_string(),
                info.base_hw_part_number().to_string(),
            ),
            Ok(info) => (
                info.current_side_or_unknown().to_string(),
                info.next_side_or_unknown().to_string(),
                info.base_hw_part_number().to_string(),
            ),
            Err(_) => (
                "unknown".to_string(),
                "unknown".to_string(),
                String::new(),
            ),
        };

        Ok(VersionInfo {
            summit_rcm: SUMMIT_RCM_VERSION.to_string(),
            nm_version,
            build,
            supplicant,
            radio_stack,
            driver,
            kernel_vermagic,
            bluez,
            u_boot: uboot,
            current_side,
            next_side,
            base_hw_part_number,
        })
    }

    async fn get_uboot_version() -> Option<String> {
        command_stdout("fw_printenv", &["-n", "version"]).await.ok()
    }

}

async fn get_cached_result<T, F, Fut>(
    cache: &'static OnceCell<CachedResult<T>>,
    load: F,
) -> Result<T>
where
    T: Clone + Send + Sync + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    match cache
        .get_or_init(|| async { load().await.map_err(|error| error.to_string()) })
        .await
    {
        Ok(value) => Ok(value.clone()),
        Err(error) => Err(anyhow!(error.clone())),
    }
}

fn parse_bluez_version_output(output: &str) -> Option<String> {
    first_non_empty_line(output)
}

fn parse_network_manager_version_output(output: &str) -> Option<String> {
    first_non_empty_line(output)
}

fn first_non_empty_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn parse_os_release_info(content: &str) -> Result<String> {
    content
        .lines()
        .find_map(|line| line.strip_prefix("VERSION="))
        .map(|value| value.trim().trim_matches('"').to_string())
        .ok_or_else(|| anyhow!("/etc/os-release missing VERSION"))
}

async fn get_os_release_info() -> Result<String> {
    let content = tokio::fs::read_to_string("/etc/os-release").await?;
    parse_os_release_info(&content)
}

async fn get_kernel_vermagic() -> Result<String> {
    let content = tokio::fs::read_to_string("/proc/sys/kernel/osrelease").await?;
    let kernel_vermagic = content.trim();
    if kernel_vermagic.is_empty() {
        return Err(anyhow!("/proc/sys/kernel/osrelease returned no data"));
    }
    Ok(kernel_vermagic.to_string())
}

async fn get_wireless_driver() -> Result<Option<String>> {
    let mut entries = tokio::fs::read_dir("/sys/class/net").await?;
    while let Some(entry) = entries.next_entry().await? {
        let interface = entry.file_name();
        let interface = interface.to_string_lossy().into_owned();
        if !tokio::fs::try_exists(entry.path().join("wireless")).await? {
            continue;
        }
        let driver_link = Path::new("/sys/class/net")
            .join(&interface)
            .join("device/driver");
        let Ok(driver_path) = tokio::fs::read_link(&driver_link).await else {
            continue;
        };
        let Some(driver_name) = driver_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !driver_name.is_empty() {
            return Ok(Some(driver_name.to_string()));
        }
    }

    Ok(None)
}

async fn get_bluez_version() -> Option<String> {
    let path = "/usr/libexec/bluetooth/bluetoothd";
    if !Path::new(path).exists() {
        return Some("Unknown".to_string());
    }

    let text = command_stdout(path, &["-v"]).await.ok()?;
    parse_bluez_version_output(&text)
}

pub(crate) async fn get_network_manager_version() -> Result<String> {
    let text = command_stdout("NetworkManager", &["--version"]).await?;
    parse_network_manager_version_output(&text)
        .ok_or_else(|| anyhow!("NetworkManager --version returned no version output"))
}

async fn get_supplicant_version() -> Result<String> {
    let text = match command_stdout("sdcsupp", &["-v"]).await {
        Ok(text) => text,
        Err(_) => command_stdout("wpa_supplicant", &["-v"]).await?,
    };
    Ok(parse_supplicant_version_output(&text).unwrap_or_default())
}

fn parse_supplicant_version_output(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| line.split_once(' ').map(|(_, rest)| rest.trim()))
        .and_then(|rest| rest.strip_prefix('v'))
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/version_service.rs"]
mod tests;
