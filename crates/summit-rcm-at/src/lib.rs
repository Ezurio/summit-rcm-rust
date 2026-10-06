//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub mod commands;
pub(crate) mod connection_service;
pub mod data_mode;
pub mod fsm;
pub(crate) mod http_connector;
pub(crate) mod http_service;
pub(crate) mod ssl;

#[doc(hidden)]
pub use inventory::submit as __inventory_submit;

#[macro_export]
macro_rules! declare_at_api {
    ($commands:expr $(,)?) => {
        #[cfg(feature = "at-interface")]
        $crate::__inventory_submit! {
            $crate::commands::AtCommandSetRegistration($commands)
        }
    };
}

pub async fn run(shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
    let serial_port =
        summit_rcm_core::config::ServerConfig::get_string("summit-rcm", "serial_port", "")
            .trim()
            .to_string();
    let baud_rate = summit_rcm_core::config::ServerConfig::get_u32("summit-rcm", "baud_rate", 0);

    if serial_port.is_empty() || baud_rate == 0 {
        log::warn!("AT interface enabled but serial port configuration is missing; nothing to do");
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
