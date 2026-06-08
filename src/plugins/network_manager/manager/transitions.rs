//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::Result;
use crate::dbus::DBUS_PROP_IFACE;
use futures_util::StreamExt;
use std::time::Duration;
use zbus::MessageStream;

use super::{
    NetworkManagerService, NM_BUS_NAME, NM_IFACE, NM_MAIN_OBJ,
    NM_SETTINGS_CONNECTION_IFACE,
};

const NETWORK_STATE_VERIFY_TIMEOUT: Duration = Duration::from_secs(3);

impl NetworkManagerService {
    async fn subscribe_to_nm_properties_changed(
        max_queued: usize,
    ) -> Result<MessageStream> {
        crate::dbus::subscribe_to_signal(
            NM_BUS_NAME,
            DBUS_PROP_IFACE,
            "PropertiesChanged",
            max_queued,
        )
        .await
    }

    async fn subscribe_to_connection_removed() -> Result<MessageStream> {
        crate::dbus::subscribe_to_signal(NM_BUS_NAME, NM_SETTINGS_CONNECTION_IFACE, "Removed", 16).await
    }

    /// Drive `stream` until `is_satisfied` reports the target state, bounded by
    /// [`NETWORK_STATE_VERIFY_TIMEOUT`]. Only `PropertiesChanged` signals on the
    /// main NetworkManager object that touch `watched_property` trigger a
    /// re-check; `timeout_message` describes the awaited transition for the
    /// timeout error.
    async fn wait_for_nm_main_property<C, Fut>(
        stream: &mut MessageStream,
        watched_property: &str,
        timeout_message: String,
        mut is_satisfied: C,
    ) -> Result<()>
    where
        C: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<bool>>,
    {
        tokio::time::timeout(NETWORK_STATE_VERIFY_TIMEOUT, async {
            loop {
                if is_satisfied().await? {
                    return Ok(());
                }

                let Some(message) = stream.next().await else {
                    anyhow::bail!("NetworkManager PropertiesChanged stream ended unexpectedly");
                };
                let message = message?;

                let Some(signal) = crate::dbus::parse_properties_changed(&message) else {
                    continue;
                };

                if signal.path != NM_MAIN_OBJ || signal.interface != NM_IFACE {
                    continue;
                }

                let _ = signal.changed.contains_key(watched_property)
                    || signal
                        .invalidated
                        .iter()
                        .any(|property| property == watched_property);
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("{timeout_message}"))??;

        Ok(())
    }

    async fn wait_for_active_connection_state(
        uuid: &str,
        active: bool,
        stream: &mut MessageStream,
    ) -> Result<()> {
        let timeout_message = format!(
            "Timed out waiting for connection '{}' to {}",
            uuid,
            if active { "activate" } else { "deactivate" }
        );
        Self::wait_for_nm_main_property(stream, "ActiveConnections", timeout_message, || async {
            Ok(Self::get_active_connection_path_by_uuid(uuid).await?.is_some() == active)
        })
        .await
    }

    async fn wait_for_connection_removed(
        uuid: &str,
        connection_obj_path: &str,
        stream: &mut MessageStream,
    ) -> Result<()> {
        tokio::time::timeout(NETWORK_STATE_VERIFY_TIMEOUT, async {
            loop {
                if Self::get_connection_path_by_uuid(uuid).await.is_err() {
                    return Ok(());
                }

                let Some(message) = stream.next().await else {
                    anyhow::bail!("NetworkManager Removed stream ended unexpectedly");
                };
                let message = message?;

                let path_matches = message
                    .header()
                    .path()
                    .map(|path| path.as_str() == connection_obj_path)
                    .unwrap_or(false);
                if !path_matches {
                    continue;
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("Timed out waiting for connection '{}' to be deleted", uuid))??;

        Ok(())
    }

    async fn wait_for_wifi_enabled_state(enabled: bool, stream: &mut MessageStream) -> Result<()> {
        let timeout_message = format!(
            "Timed out waiting for Wi-Fi to be {}",
            if enabled { "enabled" } else { "disabled" }
        );
        Self::wait_for_nm_main_property(stream, "WirelessEnabled", timeout_message, || async {
            Ok(Self::get_wifi_enabled_dbus().await? == enabled)
        })
        .await
    }

    pub async fn activate_connection_and_wait(
        uuid: &str,
        connection_obj_path: &str,
        device_obj_path: Option<&str>,
    ) -> Result<()> {
        let mut active_changes = Self::subscribe_to_nm_properties_changed(16).await?;
        Self::activate_connection_dbus(connection_obj_path, device_obj_path).await?;
        Self::wait_for_active_connection_state(uuid, true, &mut active_changes).await
    }

    pub async fn deactivate_connection_and_wait(uuid: &str) -> Result<()> {
        let Some(active_path) = Self::get_active_connection_path_by_uuid(uuid).await? else {
            return Ok(());
        };

        let mut active_changes = Self::subscribe_to_nm_properties_changed(16).await?;
        Self::deactivate_connection_dbus(active_path.as_str()).await?;
        Self::wait_for_active_connection_state(uuid, false, &mut active_changes).await
    }

    pub async fn delete_connection_and_wait(uuid: &str) -> Result<()> {
        let connection_path = Self::get_connection_path_by_uuid(uuid).await?;
        Self::deactivate_connection_and_wait(uuid).await?;

        let mut removed_signals = Self::subscribe_to_connection_removed().await?;
        Self::delete_connection_dbus(connection_path.as_str()).await?;
        Self::wait_for_connection_removed(uuid, connection_path.as_str(), &mut removed_signals).await
    }

    pub async fn set_wifi_enabled_and_wait(enabled: bool) -> Result<()> {
        let mut main_changes = Self::subscribe_to_nm_properties_changed(16).await?;
        Self::set_wifi_enabled_dbus(enabled).await?;
        Self::wait_for_wifi_enabled_state(enabled, &mut main_changes).await
    }
}
