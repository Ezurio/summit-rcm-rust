//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Log management service (journalctl / supplicant / Wi-Fi driver)

use summit_rcm_core::config::SystemSettingsManage;
use crate::{CURRENT_PROCESS_LOG_IDENTIFIER, DriverLogLevel, JournalctlLogType, SupplicantLogLevel};
use summit_rcm_network::service::NetworkService;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
#[cfg(any(feature = "api-v2", feature = "at-interface"))]
use std::sync::Mutex;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{Duration, OffsetDateTime, UtcOffset};
use tokio::process::Command;
use log::error;

#[cfg(any(feature = "api-v2", feature = "at-interface"))]
pub(crate) const VALID_WEBSERVER_LOG_LEVELS: &[&str] =
    &["critical", "error", "warning", "info", "debug", "trace"];

#[cfg(any(feature = "api-v2", feature = "at-interface"))]
static WEBSERVER_LOG_LEVEL: Mutex<&'static str> = Mutex::new("error");
static JOURNALCTL_DAYS_SINCE_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
static JOURNALCTL_LOG_ENTRY_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:6]");

pub(crate) struct LogsService;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct JournalLogEntry {
    pub time: String,
    pub priority: String,
    pub identifier: String,
    pub message: serde_json::Value,
}

impl LogsService {
    // ------------------------------------------------------------------ journal

    fn local_offset() -> UtcOffset {
        UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
    }

    fn format_days_since_for_journalctl(days: u32, hours: u32) -> Result<String> {
        let timestamp = OffsetDateTime::now_utc()
            .to_offset(Self::local_offset())
            .saturating_sub(Duration::days(i64::from(days)))
            .saturating_sub(Duration::hours(i64::from(hours)));
        Ok(timestamp.format(JOURNALCTL_DAYS_SINCE_FORMAT)?)
    }

    fn journalctl_identifier(log_type: JournalctlLogType) -> Option<&'static str> {
        match log_type {
            JournalctlLogType::All => None,
            JournalctlLogType::Kernel => Some("kernel"),
            JournalctlLogType::NetworkManager => Some("NetworkManager"),
            JournalctlLogType::CurrentProcess => Some(CURRENT_PROCESS_LOG_IDENTIFIER),
            JournalctlLogType::AdaptiveWw => Some("adaptive_ww"),
        }
    }

    fn format_log_timestamp(timestamp: &str) -> String {
        let micros = match timestamp.parse::<i128>() {
            Ok(value) => value,
            Err(_) => return "Undefined".to_string(),
        };
        let nanos = match micros.checked_mul(1000) {
            Some(value) => value,
            None => return "Undefined".to_string(),
        };
        let offset_time = match OffsetDateTime::from_unix_timestamp_nanos(nanos) {
            Ok(value) => value.to_offset(Self::local_offset()),
            Err(_) => return "Undefined".to_string(),
        };
        offset_time
            .format(JOURNALCTL_LOG_ENTRY_FORMAT)
            .unwrap_or_else(|_| "Undefined".to_string())
    }

    pub(crate) async fn get_journal_log_data(
        log_type: JournalctlLogType,
        priority: u8,
        days: u32,
        hours: u32,
    ) -> Result<Vec<JournalLogEntry>> {
        if priority > 7 {
            anyhow::bail!("Priority must be an int between 0-7");
        }

        let mut args = vec![format!("--priority={priority}"), "--output=json".to_string()];

        if let Some(identifier) = Self::journalctl_identifier(log_type) {
            args.push(format!("--identifier={identifier}"));
        }
        if days > 0 || hours > 0 {
            args.push(format!(
                "--since={}",
                Self::format_days_since_for_journalctl(days, hours)?
            ));
        }

        let output = Command::new("journalctl").args(&args).output().await?;
        if !output.status.success() {
            anyhow::bail!(
                "journalctl failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let mut logs = Vec::with_capacity(SystemSettingsManage::get_int("log_data_streaming_size", 100).max(1) as usize);
        for line in stdout_str.lines() {
            if line.trim().is_empty() {
                break;
            }

            let entry: serde_json::Value = serde_json::from_str(line)?;
            let timestamp = entry
                .get("__REALTIME_TIMESTAMP")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Undefined");
            logs.push(JournalLogEntry {
                time: Self::format_log_timestamp(timestamp),
                priority: entry
                    .get("PRIORITY")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("7")
                    .to_string(),
                identifier: entry
                    .get("SYSLOG_IDENTIFIER")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Undefined")
                    .to_string(),
                message: entry
                    .get("MESSAGE")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::String("Undefined".to_string())),
            });
        }

        Ok(logs)
    }

    // ---------------------------------------------------------------- supplicant

    pub(crate) async fn try_get_supplicant_debug_level() -> Result<SupplicantLogLevel> {
        let value = NetworkService::get_supplicant_debug_level().await?;
        SupplicantLogLevel::from_str(value.trim()).map_err(anyhow::Error::msg)
    }

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub(crate) async fn get_supplicant_debug_level() -> SupplicantLogLevel {
        Self::try_get_supplicant_debug_level()
            .await
            .unwrap_or(SupplicantLogLevel::Info)
    }

    pub(crate) async fn set_supplicant_debug_level(level: SupplicantLogLevel) -> Result<()> {
        let level_str = format!("{:?}", level).to_lowercase();
        NetworkService::set_supplicant_debug_level(&level_str).await
    }

    // -------------------------------------------------------------- Wi-Fi driver

    pub(crate) async fn try_get_wifi_driver_debug_level() -> Result<DriverLogLevel> {
        Ok(match NetworkService::get_wifi_driver_debug_level().await? {
            0 => DriverLogLevel::Disabled,
            _ => DriverLogLevel::Enabled,
        })
    }

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub(crate) async fn get_wifi_driver_debug_level() -> DriverLogLevel {
        Self::try_get_wifi_driver_debug_level()
            .await
            .unwrap_or(DriverLogLevel::Disabled)
    }

    pub(crate) async fn set_wifi_driver_debug_level(level: DriverLogLevel) {
        let value = match level {
            DriverLogLevel::Disabled => 0,
            DriverLogLevel::Enabled => 1,
        };
        if let Err(error) = NetworkService::set_wifi_driver_debug_level(value).await {
            error!("Failed to set Wi-Fi driver debug level: {}", error);
        }
    }

    // ------------------------------------------------------------ webserver log

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub(crate) fn get_webserver_log_level() -> String {
        WEBSERVER_LOG_LEVEL.lock().unwrap().to_string()
    }

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub(crate) fn set_webserver_log_level(level: &str) {
        if let Some(&l) = VALID_WEBSERVER_LOG_LEVELS.iter().find(|&&l| l == level) {
            *WEBSERVER_LOG_LEVEL.lock().unwrap() = l;
        }
    }
}
