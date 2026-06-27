//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Service for querying and setting FIPS mode via the `fips-set` script

use anyhow::Result;
use summit_rcm_core::utils::command_output;
use summit_rcm_core::utils::read_sysfs;
use log::error;

const FIPS_SCRIPT: &str = "/usr/bin/fips-set";
const FIPS_ENABLED_PATH: &str = "/proc/sys/crypto/fips_enabled";
const FIPS_WIFI_ENABLED_PATH: &str = "/proc/sys/crypto/fips_wifi_enabled";
const VALID_STATES: &[&str] = &["fips", "fips_wifi", "unset"];

enum FipsScriptResult {
	Success,
	Missing,
	Failure,
}

pub(crate) struct FipsService;

impl FipsService {
	async fn read_fips_flag(path: &str) -> Option<bool> {
		match read_sysfs(path).await {
			Ok(value) => Some(value == "1"),
			Err(_) => None,
		}
	}

	async fn run_fips_script(args: &[&str]) -> FipsScriptResult {
		match command_output(FIPS_SCRIPT, args).await {
			Ok(output) if output.status.success() => FipsScriptResult::Success,
			Ok(_) => FipsScriptResult::Failure,
			Err(error)
				if error
					.downcast_ref::<std::io::Error>()
					.is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound) => {
				FipsScriptResult::Missing
			}
			Err(_) => FipsScriptResult::Failure,
		}
	}

	pub(crate) async fn get_fips_state() -> &'static str {
		let fips_enabled = match Self::read_fips_flag(FIPS_ENABLED_PATH).await {
			Some(enabled) => enabled,
			None => return "unsupported",
		};

		if !fips_enabled {
			"unset"
		} else {
			match Self::read_fips_flag(FIPS_WIFI_ENABLED_PATH).await {
				Some(true) => "fips_wifi",
				Some(false) | None => "fips",
			}
		}
	}

	pub(crate) async fn set_fips_state(value: &str) -> Result<bool> {
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
			FipsScriptResult::Failure => {
				error!("set_fips_state: fips-set command failed");
				Ok(false)
			}
		}
	}
}
