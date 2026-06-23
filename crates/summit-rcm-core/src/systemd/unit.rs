//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Helper for interacting with systemd units via D-Bus (zbus)

use crate::dbus;
use crate::dbus::DBUS_PROP_IFACE;
use anyhow::{bail, Context, Result};
use log::error;
use std::collections::HashMap;
use zbus::zvariant::OwnedObjectPath;
use zbus::zvariant::OwnedValue;

pub const SYSTEMD_BUS_NAME: &str = "org.freedesktop.systemd1";
const SYSTEMD_MAIN_OBJ: &str = "/org/freedesktop/systemd1";
const SYSTEMD_MANAGER_IFACE: &str = "org.freedesktop.systemd1.Manager";
pub const SYSTEMD_UNIT_IFACE: &str = "org.freedesktop.systemd1.Unit";

/// Valid systemd unit configuration states (used by stunnel and log-forwarding plugins)
pub const SYSTEMD_UNIT_VALID_CONFIG_STATES: &[&str] = &["active", "inactive"];

pub struct SystemdUnit {
    pub unit_file: String,
}

impl SystemdUnit {
    pub fn new(unit_file: impl Into<String>) -> Self {
        Self { unit_file: unit_file.into() }
    }

    pub async fn unit_path(&self) -> Result<OwnedObjectPath> {
        let conn = dbus::system_bus().await?;
        self.unit_path_with_conn(conn.as_ref()).await
    }

    async fn unit_path_with_conn(&self, conn: &zbus::Connection) -> Result<OwnedObjectPath> {
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

    /// Retrieve the current ActiveState string for the unit using legacy
    /// WebLCM semantics, which treat a unit that is not loaded by systemd as
    /// an unknown/error state instead of a successful inactive state.
    pub async fn try_get_active_state_legacy(&self) -> Result<String> {
        self.query_active_state_legacy().await
    }

    fn active_state_from_properties(unit_properties: &HashMap<String, OwnedValue>) -> String {
        dbus::property::<String>(unit_properties, "ActiveState").unwrap_or_default()
    }

    fn active_state_from_properties_legacy(unit_properties: &HashMap<String, OwnedValue>) -> String {
        if dbus::property::<String>(unit_properties, "LoadState").as_deref() == Some("not-found") {
            return "unknown".to_string();
        }
        Self::active_state_from_properties(unit_properties)
    }

    async fn query_active_state(&self) -> Result<String> {
        let unit_properties = self.query_unit_properties().await?;
        Ok(Self::active_state_from_properties(&unit_properties))
    }

    async fn query_active_state_legacy(&self) -> Result<String> {
        let unit_properties = self.query_unit_properties().await?;
        Ok(Self::active_state_from_properties_legacy(&unit_properties))
    }

    async fn query_unit_properties(&self) -> Result<HashMap<String, OwnedValue>> {
        let conn = dbus::system_bus().await?;
        let unit_path = self.unit_path_with_conn(conn.as_ref()).await?;

        let unit_properties: HashMap<String, OwnedValue> =
            dbus::call_method_deserialize_with_timeout(
                conn.as_ref(),
                Some(SYSTEMD_BUS_NAME),
                unit_path.as_str(),
                Some(DBUS_PROP_IFACE),
                "GetAll",
                &(SYSTEMD_UNIT_IFACE,),
                None,
            )
            .await?;

        Ok(unit_properties)
    }

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

    pub async fn start(&self) -> Result<()> {
        self.unit_action_result("StartUnit").await
    }

    pub async fn stop(&self) -> Result<()> {
        self.unit_action_result("StopUnit").await
    }

    pub async fn restart(&self) -> Result<()> {
        self.unit_action_result("RestartUnit").await
    }

    /// Transition to the requested state. Returns Ok(()) on success.
    /// Returns Err if already in that state or on failure.
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

#[cfg(test)]
#[path = "../../tests/systemd/unit/tests.rs"]
mod tests;
