//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Shared D-Bus test seam.
//!
//! These helpers let downstream test targets override the system-bus address
//! that [`crate::dbus::system_bus`] resolves, without mutating process
//! environment variables inside tests.

use std::sync::{LazyLock, Mutex};

static SYSTEM_BUS_ADDRESS_OVERRIDE: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn system_bus_address_override() -> Option<String> {
    SYSTEM_BUS_ADDRESS_OVERRIDE.lock().unwrap().clone()
}

pub fn set_system_bus_address_override(address: &str) {
    *SYSTEM_BUS_ADDRESS_OVERRIDE.lock().unwrap() = Some(address.to_string());
}

pub fn clear_system_bus_address_override() {
    *SYSTEM_BUS_ADDRESS_OVERRIDE.lock().unwrap() = None;
}
