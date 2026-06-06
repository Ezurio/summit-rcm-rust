//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

use serde::{Deserialize, Serialize};

pub(crate) const FACTORY_RESET_SCRIPT: &str = "/usr/sbin/do_factory_reset.sh";

/// Enumeration of valid power states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum PowerState {
	On,
	Off,
	Suspend,
	Reboot,
}

impl std::fmt::Display for PowerState {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::On => write!(f, "on"),
			Self::Off => write!(f, "off"),
			Self::Suspend => write!(f, "suspend"),
			Self::Reboot => write!(f, "reboot"),
		}
	}
}

impl std::str::FromStr for PowerState {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"on" => Ok(Self::On),
			"off" => Ok(Self::Off),
			"suspend" => Ok(Self::Suspend),
			"reboot" => Ok(Self::Reboot),
			_ => Err(format!("Unknown power state: {}", s)),
		}
	}
}

#[cfg(feature = "at-interface")]
pub mod at_commands;
pub mod system_service;
pub mod version_service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod routes;

pub use self::system_service::SystemService;
pub use self::version_service::VersionService;

crate::declare_plugin_api! {
	routes {
		v2 => [
			unprov_protected "/api/v2/system/version" => {
				GET => routes::v2::version::get_version
			},
			unprov_protected "/api/v2/system/power" => {
				GET => routes::v2::power::get_power,
				PUT => routes::v2::power::set_power
			},
			protected "/api/v2/system/factoryReset" => {
				PUT => routes::v2::factory_reset::factory_reset_put
			},
			protected "/api/v2/system/debug/export" => {
				GET => routes::v2::debug::get_debug_export
			},
		],
		legacy => [
			public "/definitions" => {
				GET => routes::legacy::definitions::get_definitions
			},
			unprov_public "/version" => {
				GET => routes::legacy::version::get_version_legacy
			},
			unprov_protected "/poweroff" => {
				PUT => routes::legacy::advanced::poweroff_legacy
			},
			protected "/suspend" => {
				PUT => routes::legacy::advanced::suspend_legacy
			},
			unprov_protected "/reboot" => {
				PUT => routes::legacy::advanced::reboot_legacy
			},
			protected "/factoryReset" => {
				PUT => routes::legacy::advanced::factory_reset_legacy
			},
		],
	},
}

crate::declare_plugin! {
	cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"));
	name: "system",
	at_commands: at_commands::COMMANDS,
}

