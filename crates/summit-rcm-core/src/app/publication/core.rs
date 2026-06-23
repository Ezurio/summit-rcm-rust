//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Plugin lifecycle registry.
//!
//! Core only collects plugin **startup hooks**. Route/OpenAPI publication is
//! owned by the `summit-rcm-web` crate and AT command publication by the
//! `summit-rcm-at` crate, each through their own `inventory` registry, so core
//! knows nothing about HTTP or AT command types.

/// Self-registration handle for a plugin's startup hook.
///
/// Plugins that need one-time initialization at daemon start submit one of
/// these via `inventory`; the binary runs every collected hook before spawning
/// the interface tasks.
pub struct StartupHook(pub fn());

inventory::collect!(StartupHook);

/// All registered plugin startup hooks.
pub fn startup_hooks() -> Vec<fn()> {
    inventory::iter::<StartupHook>()
        .map(|registration| registration.0)
        .collect()
}

/// Register a plugin startup hook with the core lifecycle registry.
///
/// `$hook` is a `fn()` run once at daemon start before the interface tasks are
/// spawned. Startup is a core lifecycle concern, independent of the web or AT
/// subsystems, so plugins register it directly through core.
#[macro_export]
macro_rules! declare_startup {
    ($hook:expr) => {
        $crate::__inventory_submit! {
            $crate::StartupHook($hook)
        }
    };
}
