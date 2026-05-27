//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Helper for interacting with systemd units via D-Bus (zbus)

use crate::dbus;
use anyhow::Result;
#[cfg(any(feature = "stunnel", feature = "log-forwarding"))]
use anyhow::{bail, Context};
use log::error;
use zbus::zvariant::OwnedObjectPath;

pub const SYSTEMD_BUS_NAME: &str = "org.freedesktop.systemd1";
const SYSTEMD_MAIN_OBJ: &str = "/org/freedesktop/systemd1";
const SYSTEMD_MANAGER_IFACE: &str = "org.freedesktop.systemd1.Manager";
pub const SYSTEMD_UNIT_IFACE: &str = "org.freedesktop.systemd1.Unit";

/// Valid systemd unit configuration states (used by stunnel and log-forwarding plugins)
#[cfg(all(
    any(feature = "api-v2", feature = "api-legacy"),
    any(feature = "stunnel", feature = "log-forwarding")
))]
pub const SYSTEMD_UNIT_VALID_CONFIG_STATES: &[&str] = &["active", "inactive"];

pub struct SystemdUnit {
    pub unit_file: String,
}

impl SystemdUnit {
    #[cfg(any(
        feature = "network",
        feature = "api-v2",
        feature = "api-legacy",
        all(feature = "provisioning", any(feature = "api-v2", feature = "api-legacy")),
        feature = "log-forwarding",
        feature = "stunnel"
    ))]
    pub fn new(unit_file: impl Into<String>) -> Self {
        Self { unit_file: unit_file.into() }
    }

    pub async fn unit_path(&self) -> Result<OwnedObjectPath> {
        let conn = dbus::system_bus().await?;

        dbus::call_method_deserialize_with_timeout(
            conn,
            Some(SYSTEMD_BUS_NAME),
            SYSTEMD_MAIN_OBJ,
            Some(SYSTEMD_MANAGER_IFACE),
            "LoadUnit",
            &(&self.unit_file,),
            None,
        )
        .await
    }

    /// Retrieve the current ActiveState string for the unit
    pub async fn get_active_state(&self) -> String {
        match self.query_active_state().await {
            Ok(s) => s,
            Err(e) => {
                error!("Could not read ActiveState of {}: {}", self.unit_file, e);
                "unknown".to_string()
            }
        }
    }

    /// Retrieve the current ActiveState string for the unit.
    ///
    /// Unlike `get_active_state`, this does not swallow D-Bus errors.
    pub async fn try_get_active_state(&self) -> Result<String> {
        self.query_active_state().await
    }

    async fn query_active_state(&self) -> Result<String> {
        let conn = dbus::system_bus().await?;
        let unit_path = self.unit_path().await?;

        // Match Python baseline behavior: if systemd cannot load the unit,
        // surface this as an unknown state instead of inactive.
        let load_state = dbus::get_property_with_timeout::<String>(
            conn.clone(),
            SYSTEMD_BUS_NAME,
            unit_path.as_str(),
            SYSTEMD_UNIT_IFACE,
            "LoadState",
            None,
        )
        .await?;
        if load_state == "not-found" {
            return Ok("unknown".to_string());
        }

        let state = dbus::get_property_with_timeout::<String>(
            conn,
            SYSTEMD_BUS_NAME,
            unit_path.as_str(),
            SYSTEMD_UNIT_IFACE,
            "ActiveState",
            None,
        )
        .await?;

        Ok(state)
    }

    #[cfg(any(
        feature = "stunnel",
        feature = "log-forwarding",
        all(feature = "provisioning", any(feature = "api-v2", feature = "api-legacy"))
    ))]
    async fn unit_action_result(&self, method: &str) -> Result<()> {
        let conn = dbus::system_bus().await?;
        dbus::call_method(
            conn,
            Some(SYSTEMD_BUS_NAME),
            SYSTEMD_MAIN_OBJ,
            Some(SYSTEMD_MANAGER_IFACE),
            method,
            &(&self.unit_file, "replace"),
            None,
        )
        .await?;
        Ok(())
    }

    #[cfg(any(feature = "stunnel", feature = "log-forwarding"))]
    pub async fn start(&self) -> Result<()> {
        self.unit_action_result("StartUnit").await
    }

    #[cfg(any(feature = "stunnel", feature = "log-forwarding"))]
    pub async fn stop(&self) -> Result<()> {
        self.unit_action_result("StopUnit").await
    }

    #[cfg(all(feature = "provisioning", any(feature = "api-v2", feature = "api-legacy")))]
    pub async fn restart(&self) -> Result<()> {
        self.unit_action_result("RestartUnit").await
    }

    /// Transition to the requested state. Returns Ok(()) on success.
    /// Returns Err if already in that state or on failure.
    #[cfg(any(feature = "stunnel", feature = "log-forwarding"))]
    pub async fn set_state(&self, requested: &str) -> Result<()> {
        let current = self.get_active_state().await;
        match requested {
            "active" => {
                if current == "active" { bail!("already active"); }
                self.start().await.context("Activation failed")?;
            }
            "inactive" => {
                if current == "inactive" { bail!("already inactive"); }
                self.stop().await.context("Deactivation failed")?;
            }
            _ => {}
        }
        Ok(())
    }
}
