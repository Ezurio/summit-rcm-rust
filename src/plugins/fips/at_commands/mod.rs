//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! FIPS AT command: at+fips

use crate::at_interface::commands::{CommandOutcome, PublishedCommand};
use crate::at_interface::commands::params::CsvParams;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::fips::service::FipsService;
use crate::plugins::system::PowerState;
use log::error;

pub async fn execute_fips(_fsm: &FsmHandle, params: &CsvParams<'_>) -> CommandOutcome {
	match params.parameter_count() {
		0 => {
			let fips = FipsService::get_fips_state().await;
			CommandOutcome::WithData(format!("+FIPS: {}", fips))
		}
		1 => {
			let state_str = params.trimmed(0);
			if state_str.is_empty() {
				let fips = FipsService::get_fips_state().await;
				return CommandOutcome::WithData(format!("+FIPS: {}", fips));
			}

			// Python parity: state requires reboot parameter when provided.
			CommandOutcome::Error
		}
		2 => {
			let state_name = match params.trimmed(0).parse::<i32>() {
				Ok(0) => "fips",
				Ok(1) => "fips_wifi",
				Ok(2) => "unset",
				_ => return CommandOutcome::Error,
			};

			let reboot_param = params.trimmed(1);
			let reboot = if reboot_param.is_empty() {
				false
			} else {
				match reboot_param.parse::<i32>() {
					Ok(value) => value != 0,
					Err(_) => return CommandOutcome::Error,
				}
			};

			match FipsService::set_fips_state(state_name).await {
				Ok(success) => {
					if !success {
						return CommandOutcome::Error;
					}
					if reboot {
						let _ = crate::plugins::system::SystemService::set_power_state(PowerState::Reboot).await;
					}
					CommandOutcome::Ok
				}
				Err(error) => {
					error!("FIPS set error: {}", error);
					CommandOutcome::Error
				}
			}
		}
		_ => CommandOutcome::Error,
	}
}

pub(crate) const COMMANDS: &[PublishedCommand] = &[
	crate::at_interface::commands::command_spec!(
		"at+fips",
		"AT+FIPS[=<state>[,<reboot>]]  (state: 0=fips, 1=fips_wifi, 2=unset)",
		0,
		&[],
		execute_fips
	),
];

