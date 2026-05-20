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
const VALID_STATES: &[&str] = &["fips", "fips_wifi", "unset"];

enum FipsScriptResult {
	Success(Output),
	Missing,
	Failure(String),
}

pub struct FipsService;

impl FipsService {
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
			Ok(output) if output.status.success() => FipsScriptResult::Success(output),
			Ok(output) => FipsScriptResult::Failure(Self::command_error_message(&output)),
			Err(error)
				if error
					.downcast_ref::<std::io::Error>()
					.is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
			{
				FipsScriptResult::Missing
			}
			Err(error) => FipsScriptResult::Failure(error.to_string()),
		}
	}

	pub async fn get_fips_state() -> String {
		match Self::run_fips_script(&["status"]).await {
			FipsScriptResult::Success(output) => {
				let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
				if VALID_STATES.contains(&state.as_str()) {
					state
				} else {
					"unknown".to_string()
				}
			}
			FipsScriptResult::Missing => "unsupported".to_string(),
			FipsScriptResult::Failure(_) => "unknown".to_string(),
		}
	}

	pub async fn set_fips_state(value: &str) -> Result<bool> {
		if !VALID_STATES.contains(&value) {
			return Err(anyhow::anyhow!("invalid input parameter {}", value));
		}
		match Self::run_fips_script(&[value]).await {
			FipsScriptResult::Success(_) => Ok(true),
			FipsScriptResult::Missing => Ok(false),
			FipsScriptResult::Failure(message) => {
				error!("set_fips_state: {}", message);
				Ok(false)
			}
		}
	}
}