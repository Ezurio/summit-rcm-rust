//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub use summit_rcm_core::definition::CURRENT_PROCESS_LOG_IDENTIFIER;

mod logs_service;

#[cfg(feature = "at-interface")]
mod at_commands;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
mod routes;

/// Enumeration of valid journalctl log identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub enum JournalctlLogType {
    #[serde(rename = "kernel")]
    Kernel,
    #[serde(rename = "NetworkManager", alias = "networkmanager")]
    NetworkManager,
    #[serde(rename = "summit-rcm", alias = "python", alias = "weblcm-python")]
    CurrentProcess,
    #[serde(rename = "adaptive_ww")]
    AdaptiveWw,
    #[serde(rename = "All", alias = "all")]
    All,
}

impl FromStr for JournalctlLogType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "kernel" => Ok(Self::Kernel),
            "networkmanager" => Ok(Self::NetworkManager),
            "python" | "weblcm-python" | "summit-rcm" => Ok(Self::CurrentProcess),
            value if value == CURRENT_PROCESS_LOG_IDENTIFIER => Ok(Self::CurrentProcess),
            "adaptive_ww" => Ok(Self::AdaptiveWw),
            "all" => Ok(Self::All),
            _ => Err(format!("Unknown log type: {}", s)),
        }
    }
}

/// Enumeration of valid supplicant log levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupplicantLogLevel {
    None,
    Error,
    Warning,
    Info,
    Debug,
    Msgdump,
    Excessive,
}

impl FromStr for SupplicantLogLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "error" => Ok(Self::Error),
            "warning" => Ok(Self::Warning),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "msgdump" => Ok(Self::Msgdump),
            "excessive" => Ok(Self::Excessive),
            _ => Err(format!("Unknown supplicant log level: {}", s)),
        }
    }
}

/// Enumeration of valid Wi-Fi driver log levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriverLogLevel {
    #[serde(rename = "0")]
    Disabled = 0,
    #[serde(rename = "1")]
    Enabled = 1,
}

pub(crate) use self::logs_service::{JournalLogEntry, LogsService};

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
    name: "logs",
    routes {
        v2 => [
            protected SomeProvisioning "/api/v2/system/logs/data" => {
                GET => routes::v2::get_logs_data
            },
            protected SomeProvisioning "/api/v2/system/logs/config" => {
                GET => routes::v2::get_logs_config,
                PUT => routes::v2::set_logs_config
            },
            protected SomeProvisioning "/api/v2/system/logs/webserver" => {
                GET => routes::v2::get_logs_webserver,
                PUT => routes::v2::set_logs_webserver
            },
            protected SomeProvisioning "/api/v2/system/logs/export" => {
                GET => routes::v2::export_logs
            },
        ],
        legacy => [
            protected SomeProvisioning "/logData" => {
                GET => routes::legacy::get_log_legacy
            },
            protected SomeProvisioning "/logSetting" => {
                GET => routes::legacy::get_log_setting_legacy,
                POST => routes::legacy::put_log_setting_legacy
            },
        ],
    },
}
#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
