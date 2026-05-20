//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Version information service

use anyhow::{anyhow, Result};
use crate::utils::command_stdout;
#[cfg(feature = "network-manager")]
use crate::plugins::network_manager::service::{NetworkManagerVersionInfo, NetworkService};
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
        let network_manager_info = Self::get_network_manager_version_info().await?;
        let nm_version = network_manager_info.nm_version.clone();
        let radio_stack = nm_version.split('-').next().unwrap_or("").to_string();
        let os_release = get_os_release_info().await?;
        let build = os_release.version().to_string();
        let supplicant = get_supplicant_version().await?;
        let bluez = get_bluez_version().await.unwrap_or_else(|| "n/a".to_string());
        let uboot = Self::get_uboot_version().await.unwrap_or_default();
        let (current_side, next_side, base_hw_part_number) = match get_boot_rootfs_info().await {
            Ok(info) if info.is_running_on_sd() => (
                "sd".to_string(),
                "sd".to_string(),
                info.base_hw_part_number_owned(),
            ),
            Ok(info) => (
                info.current_side_or_unknown(),
                info.next_side_or_unknown(),
                info.base_hw_part_number_owned(),
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
            driver: network_manager_info.driver,
            kernel_vermagic: network_manager_info.driver_version,
            bluez,
            u_boot: uboot,
            current_side,
            next_side,
            base_hw_part_number,
        })
    }

    #[cfg(feature = "network-manager")]
    async fn get_network_manager_version_info() -> Result<NetworkManagerVersionInfo> {
        NetworkService::get_version_info().await
    }

    #[cfg(not(feature = "network-manager"))]
    async fn get_network_manager_version_info() -> Result<NetworkManagerVersionInfo> {
        let nm_version = get_network_manager_version().await?;

        Ok(NetworkManagerVersionInfo {
            nm_version,
            ..NetworkManagerVersionInfo::default()
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BootRootfsInfo {
    root_dev_type: String,
    current_side: String,
    next_side: String,
    base_hw_part_number: String,
}

impl BootRootfsInfo {
    pub(crate) fn is_running_on_sd(&self) -> bool {
        self.root_dev_type == "SD"
    }

    pub(crate) fn current_side_option(&self) -> Option<&str> {
        match self.current_side.as_str() {
            "a" | "b" => Some(self.current_side.as_str()),
            _ => None,
        }
    }

    pub(crate) fn next_side_option(&self) -> Option<&str> {
        match self.next_side.as_str() {
            "a" | "b" => Some(self.next_side.as_str()),
            _ => None,
        }
    }

    pub(crate) fn current_side_or_unknown(&self) -> String {
        self.current_side_option().unwrap_or("unknown").to_string()
    }

    pub(crate) fn next_side_or_unknown(&self) -> String {
        self.next_side_option().unwrap_or("unknown").to_string()
    }

    pub(crate) fn base_hw_part_number_owned(&self) -> String {
        self.base_hw_part_number.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OsReleaseInfo {
    version: String,
}

impl OsReleaseInfo {
    pub(crate) fn version(&self) -> &str {
        &self.version
    }
}

fn parse_bluez_version_output(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn parse_network_manager_version_output(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn parse_boot_rootfs_info(output: &str) -> Result<BootRootfsInfo> {
    let mut root_dev_type = None;
    let mut current_side = None;
    let mut next_side = None;
    let mut base_hw_part_number = None;

    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "rootDevType" => root_dev_type = Some(value.to_string()),
            "currentSide" => current_side = Some(value.to_string()),
            "nextSide" => next_side = Some(value.to_string()),
            "baseHwPartNumber" => base_hw_part_number = Some(value.to_string()),
            _ => {}
        }
    }

    Ok(BootRootfsInfo {
        root_dev_type: root_dev_type.ok_or_else(|| anyhow!("boot-rootfs.sh output missing rootDevType"))?,
        current_side: current_side.ok_or_else(|| anyhow!("boot-rootfs.sh output missing currentSide"))?,
        next_side: next_side.ok_or_else(|| anyhow!("boot-rootfs.sh output missing nextSide"))?,
        base_hw_part_number: base_hw_part_number
            .ok_or_else(|| anyhow!("boot-rootfs.sh output missing baseHwPartNumber"))?,
    })
}

fn parse_os_release_info(content: &str) -> Result<OsReleaseInfo> {
    let version = content
        .lines()
        .find_map(|line| line.strip_prefix("VERSION="))
        .map(|value| value.trim().trim_matches('"').to_string())
        .ok_or_else(|| anyhow!("/etc/os-release missing VERSION"))?;

    Ok(OsReleaseInfo { version })
}

pub(crate) async fn get_boot_rootfs_info() -> Result<BootRootfsInfo> {
    let output = command_stdout(
        "/bin/sh",
        &[
            "-c",
            ". boot-rootfs.sh && getSide >/dev/null && base_hw=$(getBaseHwPartNumber) && printf 'rootDevType=%s\ncurrentSide=%s\nnextSide=%s\nbaseHwPartNumber=%s\n' \"$rootDevType\" \"$bootside\" unknown \"$base_hw\"",
        ],
    )
    .await?;
    let mut info = parse_boot_rootfs_info(&output)?;
    info.next_side = get_boot_rootfs_next_side().await?;
    Ok(info)
}

async fn get_boot_rootfs_next_side() -> Result<String> {
    let output = command_stdout("/bin/sh", &["-c", ". boot-rootfs.sh && nextSide"]).await?;
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("boot-rootfs.sh nextSide returned no output"))
}

async fn get_os_release_info() -> Result<OsReleaseInfo> {
    let content = tokio::fs::read_to_string("/etc/os-release").await?;
    parse_os_release_info(&content)
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
