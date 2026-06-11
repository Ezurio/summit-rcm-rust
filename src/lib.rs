//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

// Used by declare_plugin! so the inventory call resolves from $crate regardless
// of whether the macro is invoked from within summit-rcm or from an external plugin.
#[doc(hidden)]
pub use inventory::submit as __inventory_submit;

#[path = "support/definition.rs"]
pub mod definition;

#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
#[path = "support/config.rs"]
pub mod config;
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
#[path = "support/dbus.rs"]
pub mod dbus;
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
#[path = "support/archive.rs"]
pub mod archive;
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
#[path = "support/certificates.rs"]
pub mod certificates;
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
#[cfg(feature = "notifications")]
#[path = "support/notifications.rs"]
pub mod notifications;

#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
#[path = "support/utils.rs"]
pub mod utils;

#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
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
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
pub use publication::PluginPublication;
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
pub use publication::builtin_plugin_publications as all_plugin_publications;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod web;
#[cfg(feature = "at-interface")]
pub mod at_interface;
