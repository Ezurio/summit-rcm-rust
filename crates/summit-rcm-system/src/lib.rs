//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

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
mod at_commands;
mod system_service;
mod version_service;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
mod routes;

pub use self::system_service::SystemService;
pub(crate) use self::version_service::VersionService;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
    name: "system",
    routes {
        v2 => [
            protected Any "/api/v2/system/version" => {
                GET => routes::v2::version::get_version
            },
            protected Any "/api/v2/system/power" => {
                GET => routes::v2::power::get_power,
                PUT => routes::v2::power::set_power
            },
            protected SomeProvisioning "/api/v2/system/factoryReset" => {
                PUT => routes::v2::factory_reset::factory_reset_put
            },
            protected SomeProvisioning "/api/v2/system/debug/export" => {
                GET => routes::v2::debug::get_debug_export
            },
        ],
        legacy => [
            public SomeProvisioning "/definitions" => {
                GET => routes::legacy::definitions::get_definitions
            },
            public Any "/version" => {
                GET => routes::legacy::version::get_version_legacy
            },
            protected Any "/poweroff" => {
                PUT => routes::legacy::advanced::poweroff_legacy
            },
            protected SomeProvisioning "/suspend" => {
                PUT => routes::legacy::advanced::suspend_legacy
            },
            protected Any "/reboot" => {
                PUT => routes::legacy::advanced::reboot_legacy
            },
            protected SomeProvisioning "/factoryReset" => {
                PUT => routes::legacy::advanced::factory_reset_legacy
            },
        ],
    },
}
#[cfg(feature = "at-interface")]
summit_rcm_at::declare_at_api!(at_commands::COMMANDS);
