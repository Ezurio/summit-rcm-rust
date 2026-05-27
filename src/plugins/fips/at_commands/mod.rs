//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! FIPS AT command: at+fips

use crate::at_interface::commands::Command;
use crate::at_interface::fsm::FsmHandle;
use crate::plugins::fips::service::FipsService;
use crate::plugins::system::PowerState;
use crate::publication::PublishedAtCommand;
use log::error;

pub struct Fips;

impl Command for Fips {
	fn signature(&self) -> &str { "at+fips" }
	fn name(&self) -> &str { "FIPS" }
	fn usage(&self) -> &str { "AT+FIPS[=<state>[,<reboot>]]  (state: 0=fips, 1=fips_wifi, 2=unset)" }

	fn execute<'a>(&'a self, _fsm: &'a FsmHandle, params: &'a str) -> futures_util::future::BoxFuture<'a, (bool, String)> {
		Box::pin(async move {
			let parts: Vec<&str> = params.splitn(2, ',').collect();
			let state_str = parts[0].trim();

			if state_str.is_empty() {
				let fips = FipsService::get_fips_state().await;
				return (true, format!("+FIPS: {}\r\nOK", fips));
			}

			let state_name = match state_str.parse::<i32>() {
				Ok(0) => "fips",
				Ok(1) => "fips_wifi",
				Ok(2) => "unset",
				_ => return (true, "ERROR".to_string()),
			};

			let reboot = parts
				.get(1)
				.and_then(|value| value.trim().parse::<i32>().ok())
				.map(|value| value != 0)
				.unwrap_or(false);

			match FipsService::set_fips_state(state_name).await {
				Ok(success) => {
					if !success {
						return (true, "ERROR".to_string());
					}
					if reboot {
						let _ = crate::plugins::system::SystemService::set_power_state(
							PowerState::Reboot,
						)
						.await;
					}
					(true, "OK".to_string())
				}
				Err(error) => {
					error!("FIPS set error: {}", error);
					(true, "ERROR".to_string())
				}
			}
		})
	}
}

pub static PUBLISHED_COMMANDS: &[PublishedAtCommand] = &[
	PublishedAtCommand::new("at+fips", "FIPS"),
];

pub fn add_at_commands(cmds: &mut Vec<Box<dyn Command>>) {
	cmds.push(Box::new(Fips));
}
