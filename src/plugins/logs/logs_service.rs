//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Log management service (journalctl / supplicant / Wi-Fi driver)

use crate::config::SystemSettingsManage;
use crate::plugins::logs::{CURRENT_PROCESS_LOG_IDENTIFIER, DriverLogLevel, JournalctlLogType, SupplicantLogLevel};
use crate::plugins::network::service::wifi_driver_debug_param;
use crate::utils::command_output;
use anyhow::Result;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::LazyLock;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{Duration, OffsetDateTime, UtcOffset};
use tokio::process::Command;
use tracing::error;

static WEBSERVER_LOG_LEVEL: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new("error".to_string()));
static JOURNALCTL_DAYS_SINCE_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
static JOURNALCTL_LOG_ENTRY_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:6]");

pub struct LogsService;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct JournalLogEntry {
    pub time: String,
    pub priority: String,
    pub identifier: String,
    pub message: serde_json::Value,
}

impl LogsService {
    async fn run_wpa_cli_log_level(args: &[&str]) -> Result<String> {
        let output = command_output("wpa_cli", args).await?;
        if !output.status.success() {
            anyhow::bail!(
                "wpa_cli log_level failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    // ------------------------------------------------------------------ journal

    fn local_offset() -> UtcOffset {
        UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
    }

    fn format_days_since_for_journalctl(days: u32) -> Result<String> {
        let timestamp = OffsetDateTime::now_utc()
            .to_offset(Self::local_offset())
            .saturating_sub(Duration::days(i64::from(days)));
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

    pub async fn get_journal_log_data(
        log_type: JournalctlLogType,
        priority: u8,
        days: u32,
    ) -> Result<Vec<JournalLogEntry>> {
        if priority > 7 {
            anyhow::bail!("Priority must be an int between 0-7");
        }

        let mut cmd = Command::new("journalctl");
        cmd.arg(format!("--priority={priority}"))
            .arg("--output=json");

        if let Some(identifier) = Self::journalctl_identifier(log_type) {
            cmd.arg(format!("--identifier={identifier}"));
        }
        if days > 0 {
            cmd.arg(format!(
                "--since={}",
                Self::format_days_since_for_journalctl(days)?
            ));
        }

        let output = cmd.output().await?;
        if !output.status.success() {
            anyhow::bail!(
                "journalctl failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let mut logs = Vec::with_capacity(
            (SystemSettingsManage::get_int("log_data_streaming_size", 100) as usize).min(32),
        );
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

    pub async fn try_get_supplicant_debug_level() -> Result<SupplicantLogLevel> {
        let text = Self::run_wpa_cli_log_level(&["log_level"]).await?;
        for line in text.to_lowercase().lines() {
            if let Ok(level) = SupplicantLogLevel::from_str(line.trim()) {
                return Ok(level);
            }
        }
        anyhow::bail!("Unable to determine supplicant debug level")
    }

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub async fn get_supplicant_debug_level() -> SupplicantLogLevel {
        Self::try_get_supplicant_debug_level()
            .await
            .unwrap_or(SupplicantLogLevel::Info)
    }

    pub async fn set_supplicant_debug_level(level: SupplicantLogLevel) -> Result<()> {
        let level_str = format!("{:?}", level).to_lowercase();
        let _ = Self::run_wpa_cli_log_level(&["log_level", level_str.as_str()]).await?;
        Ok(())
    }

    // -------------------------------------------------------------- Wi-Fi driver

    pub fn try_get_wifi_driver_debug_level() -> Result<DriverLogLevel> {
        let path = wifi_driver_debug_param();
        let value = std::fs::read_to_string(path)?;
        let parsed = value.trim().parse::<u8>()?;
        Ok(if parsed == 0 {
            DriverLogLevel::Disabled
        } else {
            DriverLogLevel::Enabled
        })
    }

    #[cfg(any(feature = "api-v2", feature = "at-interface"))]
    pub fn get_wifi_driver_debug_level() -> DriverLogLevel {
        Self::try_get_wifi_driver_debug_level().unwrap_or(DriverLogLevel::Disabled)
    }

    pub fn set_wifi_driver_debug_level(level: DriverLogLevel) {
        let path = wifi_driver_debug_param();
        let val = match level {
            DriverLogLevel::Disabled => "0",
            DriverLogLevel::Enabled => "1",
        };
        if let Err(e) = std::fs::write(path, val) {
            error!("Failed to set Wi-Fi driver debug level: {}", e);
        }
    }

    // ------------------------------------------------------------ webserver log

    pub fn get_webserver_log_level() -> String {
        WEBSERVER_LOG_LEVEL.lock().clone()
    }

    pub fn set_webserver_log_level(level: &str) {
        *WEBSERVER_LOG_LEVEL.lock() = level.to_string();
        // Re-initialise tracing filter at runtime
        // (best-effort; tracing-subscriber doesn't support live reload by default)
    }
}
