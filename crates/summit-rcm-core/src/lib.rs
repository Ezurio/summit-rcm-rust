//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

// Used by declare_plugin! so the inventory call resolves from $crate regardless
// of whether the macro is invoked from within summit-rcm or from an external plugin.
#[doc(hidden)]
pub use inventory::submit as __inventory_submit;

#[path = "support/archive.rs"]
pub mod archive;
#[path = "support/certificates.rs"]
pub mod certificates;
#[path = "support/config.rs"]
pub mod config;
#[path = "support/dbus.rs"]
pub mod dbus;
#[path = "support/definition.rs"]
pub mod definition;
#[path = "support/files_service.rs"]
pub mod files_service;
#[path = "app/publication.rs"]
pub mod publication;
#[path = "systemd/power.rs"]
pub mod systemd_power;
#[path = "systemd/unit.rs"]
pub mod systemd_unit;
#[path = "support/utils.rs"]
pub mod utils;
pub use publication::{StartupHook, startup_hooks};
