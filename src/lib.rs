//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

// Used by declare_plugin! so the inventory call resolves from $crate regardless
// of whether the macro is invoked from within summit-rcm or from an external plugin.
#[doc(hidden)]
pub use inventory::submit as __inventory_submit;

// Used by declare_plugin_api! helper macros for $crate-qualified access to axum.
#[doc(hidden)]
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub use axum as __axum;
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
#[path = "support/notifications.rs"]
#[cfg(feature = "notifications")]
pub mod notifications;
#[path = "support/utils.rs"]
pub mod utils;
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
pub use publication::PluginPublication;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub mod web;
#[cfg(feature = "at-interface")]
pub mod at_interface;

#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
pub fn builtin_plugin_publications() -> Vec<&'static PluginPublication> {
	publication::builtin_plugin_publications()
}

/// All active plugin publications (inventory-collected from all workspace members).
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
pub fn all_plugin_publications() -> Vec<&'static PluginPublication> {
	publication::builtin_plugin_publications()
}
