//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Service for querying and setting FIPS mode via the `fips-set` script

use anyhow::Result;
use crate::utils::command_output;
use std::process::Output;
use log::error;

const FIPS_SCRIPT: &str = "/usr/bin/fips-set";
const FIPS_ENABLED_PATH: &str = "/proc/sys/crypto/fips_enabled";
const FIPS_WIFI_ENABLED_PATH: &str = "/proc/sys/crypto/fips_wifi_enabled";
const VALID_STATES: &[&str] = &["fips", "fips_wifi", "unset"];

enum FipsScriptResult {
	Success,
	Missing,
	Failure(String),
}

pub struct FipsService;

impl FipsService {
	async fn read_fips_flag(path: &str) -> Option<bool> {
		match tokio::fs::read_to_string(path).await {
			Ok(value) => Some(value.trim() == "1"),
			Err(_) => None,
		}
	}

	fn command_error_message(output: &Output) -> String {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
		let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

		if !stderr.is_empty() {
			stderr
		} else if !stdout.is_empty() {
			stdout
		} else {
			"script failed".to_string()
		}
	}

	async fn run_fips_script(args: &[&str]) -> FipsScriptResult {
		match command_output(FIPS_SCRIPT, args).await {
			Ok(output) if output.status.success() => FipsScriptResult::Success,
			Ok(output) => FipsScriptResult::Failure(Self::command_error_message(&output)),
			Err(error)
				if error
					.downcast_ref::<std::io::Error>()
					.is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound) => {
				FipsScriptResult::Missing
			}
			Err(error) => FipsScriptResult::Failure(error.to_string()),
		}
	}

	pub async fn get_fips_state() -> String {
		let fips_enabled = match Self::read_fips_flag(FIPS_ENABLED_PATH).await {
			Some(enabled) => enabled,
			None => return "unsupported".to_string(),
		};

		if !fips_enabled {
			"unset".to_string()
		} else {
			match Self::read_fips_flag(FIPS_WIFI_ENABLED_PATH).await {
				Some(true) => "fips_wifi".to_string(),
				Some(false) | None => "fips".to_string(),
			}
		}
	}

	pub async fn set_fips_state(value: &str) -> Result<bool> {
		if !VALID_STATES.contains(&value) {
			return Err(anyhow::anyhow!("invalid input parameter {}", value));
		}
		match Self::read_fips_flag(FIPS_ENABLED_PATH).await {
			Some(_) => {}
			None => return Ok(false),
		}
		match Self::run_fips_script(&[value]).await {
			FipsScriptResult::Success => Ok(true),
			FipsScriptResult::Missing => Ok(false),
			FipsScriptResult::Failure(message) => {
				error!("set_fips_state: {}", message);
				Ok(false)
			}
		}
	}
}