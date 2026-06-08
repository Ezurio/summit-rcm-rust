//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub use crate::definition::CURRENT_PROCESS_LOG_IDENTIFIER;

pub mod logs_service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;
#[cfg(feature = "at-interface")]
pub mod at_commands;

/// Enumeration of valid journalctl log identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub enum JournalctlLogType {
	#[serde(rename = "kernel")]
	Kernel,
	#[serde(rename = "NetworkManager", alias = "networkmanager")]
	NetworkManager,
	#[serde(
		rename = "summit-rcm",
		alias = "python",
		alias = "weblcm-python",
		alias = "summit-rcm-rust"
	)]
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

pub use self::logs_service::{JournalLogEntry, LogsService};

crate::declare_plugin_api! {
	routes {
		v2 => [
			protected "/api/v2/system/logs/data" => {
				GET => routes::v2::get_logs_data
			},
			protected "/api/v2/system/logs/config" => {
				GET => routes::v2::get_logs_config,
				PUT => routes::v2::set_logs_config
			},
			protected "/api/v2/system/logs/webserver" => {
				GET => routes::v2::get_logs_webserver,
				PUT => routes::v2::set_logs_webserver
			},
			protected "/api/v2/system/logs/export" => {
				GET => routes::v2::export_logs
			},
		],
		legacy => [
			protected "/logData" => {
				GET => routes::legacy::get_log_legacy
			},
			protected "/logSetting" => {
				GET => routes::legacy::get_log_setting_legacy,
				POST => routes::legacy::put_log_setting_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "logs",
	at_commands: at_commands::COMMANDS,
}
