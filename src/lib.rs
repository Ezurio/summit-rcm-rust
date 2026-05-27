//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[path = "support/config.rs"]
pub mod config;
#[path = "support/dbus.rs"]
pub mod dbus;
#[path = "support/definition.rs"]
pub mod definition;
#[path = "support/archive.rs"]
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
pub mod archive;
#[path = "support/certificates.rs"]
pub mod certificates;
#[path = "support/utils.rs"]
pub mod utils;
#[path = "support/log_config.rs"]
pub mod log_config;
#[path = "app/loader.rs"]
pub mod plugin_loader;
#[path = "systemd/unit.rs"]
pub mod systemd_unit;
#[cfg(all(
	any(feature = "api-v2", feature = "api-legacy"),
	any(feature = "log-forwarding", feature = "stunnel"),
))]
#[path = "systemd/state.rs"]
pub mod systemd_state;
pub mod plugins;
#[path = "app/publication.rs"]
mod publication;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod web;
#[cfg(feature = "at-interface")]
pub mod at_interface;
