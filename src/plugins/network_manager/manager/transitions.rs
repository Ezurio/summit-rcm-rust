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
    NetworkManagerService, NmProperties, NM_BUS_NAME, NM_IFACE, NM_MAIN_OBJ,
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

    async fn wait_for_active_connection_state(
        uuid: &str,
        active: bool,
        stream: &mut MessageStream,
    ) -> Result<()> {
        tokio::time::timeout(NETWORK_STATE_VERIFY_TIMEOUT, async {
            loop {
                if Self::get_active_connection_path_by_uuid(uuid).await?.is_some() == active {
                    return Ok(());
                }

                let Some(message) = stream.next().await else {
                    anyhow::bail!("NetworkManager PropertiesChanged stream ended unexpectedly");
                };
                let message = message?;

                let path_matches = message
                    .header()
                    .path()
                    .map(|path| path.as_str() == NM_MAIN_OBJ)
                    .unwrap_or(false);
                if !path_matches {
                    continue;
                }

                let Ok((changed_interface, changed_properties, invalidated_properties)) =
                    message.body().deserialize::<(String, NmProperties, Vec<String>)>()
                else {
                    continue;
                };

                if changed_interface != NM_IFACE {
                    continue;
                }

                let active_connections_changed = changed_properties.contains_key("ActiveConnections")
                    || invalidated_properties
                        .iter()
                        .any(|property| property == "ActiveConnections");
                if !active_connections_changed {
                    continue;
                }
            }
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Timed out waiting for connection '{}' to {}",
                uuid,
                if active { "activate" } else { "deactivate" }
            )
        })??;

        Ok(())
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
        tokio::time::timeout(NETWORK_STATE_VERIFY_TIMEOUT, async {
            loop {
                if Self::get_wifi_enabled_dbus().await? == enabled {
                    return Ok(());
                }

                let Some(message) = stream.next().await else {
                    anyhow::bail!("NetworkManager PropertiesChanged stream ended unexpectedly");
                };
                let message = message?;

                let path_matches = message
                    .header()
                    .path()
                    .map(|path| path.as_str() == NM_MAIN_OBJ)
                    .unwrap_or(false);
                if !path_matches {
                    continue;
                }

                let Ok((changed_interface, changed_properties, invalidated_properties)) =
                    message.body().deserialize::<(String, NmProperties, Vec<String>)>()
                else {
                    continue;
                };

                if changed_interface != NM_IFACE {
                    continue;
                }

                let wireless_enabled_changed = changed_properties.contains_key("WirelessEnabled")
                    || invalidated_properties
                        .iter()
                        .any(|property| property == "WirelessEnabled");
                if !wireless_enabled_changed {
                    continue;
                }
            }
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Timed out waiting for Wi-Fi to be {}",
                if enabled { "enabled" } else { "disabled" }
            )
        })??;

        Ok(())
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
