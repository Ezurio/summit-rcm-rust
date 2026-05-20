//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub mod commands;
pub mod fsm;

pub async fn run(
	shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
	let serial_port = crate::config::ServerConfig::get_string("summit-rcm", "serial_port", "")
		.trim()
		.to_string();
	let baud_rate = crate::config::ServerConfig::get_u32("summit-rcm", "baud_rate", 0);

	if serial_port.is_empty() || baud_rate == 0 {
		log::warn!(
			"AT interface enabled but serial port configuration is missing; nothing to do"
		);
		return Ok(());
	}

	log::info!("Listening on {}", serial_port);

	fsm::AtInterface::start(serial_port, baud_rate, shutdown).await
}

pub fn spawn_task(
	shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
	tokio::spawn(async move { run(shutdown).await })
}
