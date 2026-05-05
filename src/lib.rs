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
pub mod archive;
#[path = "support/certificates.rs"]
pub mod certificates;
#[path = "support/utils.rs"]
pub mod utils;

#[path = "app/loader.rs"]
pub mod plugin_loader;
#[cfg(feature = "at-interface")]
pub mod at_interface;
#[cfg(feature = "at-interface")]
pub mod services;
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
pub mod publication;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod web;