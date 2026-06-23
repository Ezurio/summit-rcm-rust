//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::paths::provisioning_state_file_path;
use crate::{enable_client_pairing, service};

/// Boot-time daemon mode.
///
/// The provisioning subsystem switches the daemon's TLS material, identity,
/// and reachable route surface as a whole; transitions always cycle through
/// a process restart. The mode is read once on startup and dictates which
/// [`RouteMode`] values the publication walker admits.
///
/// [`RouteMode`]: summit_rcm_plugin_api::RouteMode
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootMode {
    /// Restricted provisioning mode before a device certificate is installed.
    Unprovisioned,
    /// Provisioning mode after a device certificate is installed but before
    /// time has been set inside its validity window.
    PartiallyProvisioned,
    /// Normal operating mode: the daemon booted fully provisioned,
    /// or client pairing is disabled (provisioning is a no-op).
    FullyProvisioned,
}

fn read_boot_mode() -> BootMode {
    if !enable_client_pairing() {
        return BootMode::FullyProvisioned;
    }
    let raw = std::fs::read_to_string(provisioning_state_file_path())
        .ok()
        .and_then(|s| s.trim().parse::<i32>().ok())
        .and_then(|v| service::ProvisioningState::try_from(v).ok())
        .unwrap_or(service::ProvisioningState::Unprovisioned);
    match raw {
        service::ProvisioningState::Unprovisioned => BootMode::Unprovisioned,
        service::ProvisioningState::PartiallyProvisioned => BootMode::PartiallyProvisioned,
        service::ProvisioningState::FullyProvisioned => BootMode::FullyProvisioned,
    }
}

/// Returns the boot-time daemon mode. Computed once on first call and cached.
pub fn current_boot_mode() -> BootMode {
    if !summit_rcm_core::config::env_or_trimmed("SUMMIT_RCM_PROVISIONING_STATE_FILE", "").is_empty() {
        return read_boot_mode();
    }

    use std::sync::OnceLock;
    static MODE: OnceLock<BootMode> = OnceLock::new();
    *MODE.get_or_init(read_boot_mode)
}
