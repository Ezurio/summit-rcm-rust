//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! System power control primitives via logind (`org.freedesktop.login1`).
//!
//! These are OS-level operations shared by any plugin that needs to power the
//! device down, reboot, or suspend it. They live in core so plugins do not have
//! to depend on each other (e.g. the FIPS plugin can reboot after a state change
//! without depending on the system plugin).

use crate::dbus;
use anyhow::Result;
use log::error;

const LOGIND_BUS_NAME: &str = "org.freedesktop.login1";
const LOGIND_MAIN_OBJ: &str = "/org/freedesktop/login1";
const LOGIND_MAIN_IFACE: &str = "org.freedesktop.login1.Manager";

async fn logind_call(member: &str) -> Result<()> {
    let conn = dbus::system_bus().await?;
    dbus::call_method(
        conn,
        Some(LOGIND_BUS_NAME),
        LOGIND_MAIN_OBJ,
        Some(LOGIND_MAIN_IFACE),
        member,
        &(false,),
        None,
    )
    .await?;
    Ok(())
}

/// Power the device off via logind.
pub async fn power_off() -> Result<()> {
    logind_call("PowerOff").await.map_err(|e| {
        error!("Poweroff cannot be initiated: {}", e);
        e
    })
}

/// Reboot the device via logind.
pub async fn reboot() -> Result<()> {
    logind_call("Reboot").await.map_err(|e| {
        error!("Reboot cannot be initiated: {}", e);
        e
    })
}

/// Suspend the device via logind.
pub async fn suspend() -> Result<()> {
    logind_call("Suspend").await.map_err(|e| {
        error!("Suspend cannot be initiated: {}", e);
        e
    })
}
