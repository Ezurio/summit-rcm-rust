//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Cache lifecycle and signal watchers that keep the NetworkManager status cache
//! up to date in response to D-Bus PropertiesChanged and systemd state changes.

use anyhow::Result;
use crate::dbus;
use crate::dbus::DBUS_PROP_IFACE;
use crate::systemd_unit::SystemdUnit;
use crate::systemd_unit::{SYSTEMD_BUS_NAME, SYSTEMD_UNIT_IFACE};
use futures_util::StreamExt;
use log::error;
use serde_json::json;
use std::sync::atomic::Ordering;

use super::super::super::{
    NetworkManagerService, NmProperties, NM_ACCESS_POINT_IFACE, NM_BUS_NAME,
    NM_CONNECTION_ACTIVE_IFACE, NM_DEVICE_IFACE, NM_DEVICE_WIRED_IFACE, NM_DEVICE_WIRELESS_IFACE,
    NM_DHCP4_CONFIG_IFACE, NM_DHCP6_CONFIG_IFACE, NM_IFACE, NM_IP4_CONFIG_IFACE,
    NM_IP6_CONFIG_IFACE, NM_MAIN_OBJ, NM_SETTINGS_CONNECTION_IFACE, NETWORK_STATUS_CACHE,
    NETWORK_STATUS_INIT_STARTED, NETWORK_STATUS_SIGNAL_TASK, NETWORK_STATUS_WATCHER,
};

const NETWORKMANAGER_SERVICE_FILE: &str = "NetworkManager.service";

impl NetworkManagerService {
    async fn clear_status_cache() {
        *NETWORK_STATUS_CACHE.write().await = json!({});
    }

    fn should_refresh_status_cache(
        path: &str,
        changed_interface: &str,
        changed_properties: &NmProperties,
        invalidated_properties: &[String],
    ) -> bool {
        if changed_interface == NM_IFACE && path == NM_MAIN_OBJ {
            return changed_properties.contains_key("Devices")
                || changed_properties.contains_key("ActiveConnections")
                || invalidated_properties.iter().any(|property| property == "Devices" || property == "ActiveConnections");
        }

        if changed_interface == NM_DEVICE_IFACE
            || changed_interface == NM_DEVICE_WIRED_IFACE
            || changed_interface == NM_DEVICE_WIRELESS_IFACE
            || changed_interface == NM_CONNECTION_ACTIVE_IFACE
            || changed_interface == NM_IP4_CONFIG_IFACE
            || changed_interface == NM_IP6_CONFIG_IFACE
            || changed_interface == NM_DHCP4_CONFIG_IFACE
            || changed_interface == NM_DHCP6_CONFIG_IFACE
            || changed_interface == NM_ACCESS_POINT_IFACE
            || changed_interface == NM_SETTINGS_CONNECTION_IFACE
        {
            return path.starts_with(NM_MAIN_OBJ);
        }

        false
    }

    async fn networkmanager_systemd_active_state() -> String {
        SystemdUnit::new(NETWORKMANAGER_SERVICE_FILE)
            .get_active_state()
            .await
    }

    async fn stop_status_watcher() {
        if let Some(handle) = NETWORK_STATUS_SIGNAL_TASK.lock().await.take() {
            handle.abort();
        }
    }

    async fn ensure_nm_status_watcher() -> Result<()> {
        if NETWORK_STATUS_SIGNAL_TASK.lock().await.is_some() {
            return Ok(());
        }

        let mut stream = crate::dbus::subscribe_to_signal(
            NM_BUS_NAME,
            DBUS_PROP_IFACE,
            "PropertiesChanged",
            64,
        )
        .await?;
        let handle = tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(message) => {
                        let Some(path) = message.header().path().map(|path| path.as_str().to_string()) else {
                            continue;
                        };

                        let Ok((changed_interface, changed_properties, invalidated_properties)) =
                            message.body().deserialize::<(String, NmProperties, Vec<String>)>()
                        else {
                            continue;
                        };

                        if !NetworkManagerService::should_refresh_status_cache(
                            &path,
                            &changed_interface,
                            &changed_properties,
                            &invalidated_properties,
                        ) {
                            continue;
                        }

                        if let Err(error) = NetworkManagerService::refresh_status_cache().await {
                            error!("failed to refresh cached NetworkManager status: {}", error);
                        }
                    }
                    Err(error) => {
                        error!("NetworkManager PropertiesChanged stream error: {}", error);
                        break;
                    }
                }
            }

            NETWORK_STATUS_SIGNAL_TASK.lock().await.take();
        });

        let mut watcher = NETWORK_STATUS_SIGNAL_TASK.lock().await;
        if watcher.is_some() {
            handle.abort();
        } else {
            *watcher = Some(handle);
        }

        Ok(())
    }

    async fn handle_networkmanager_systemd_state(new_state: &str) {
        match new_state {
            "active" => {
                if let Err(error) = Self::refresh_status_cache().await {
                    error!("failed to refresh NetworkManager status cache on service activation: {}", error);
                }
                if let Err(error) = Self::ensure_nm_status_watcher().await {
                    error!("failed to start NetworkManager status watcher on service activation: {}", error);
                }
            }
            "deactivating" | "inactive" | "failed" => {
                Self::stop_status_watcher().await;
                Self::clear_status_cache().await;
            }
            _ => {}
        }
    }

    async fn spawn_systemd_status_watcher() -> Result<()> {
        let unit = SystemdUnit::new(NETWORKMANAGER_SERVICE_FILE);
        let unit_path = unit.unit_path().await?;
        let unit_path = unit_path.as_str().to_string();
        let mut stream = dbus::subscribe_to_signal(
            SYSTEMD_BUS_NAME,
            DBUS_PROP_IFACE,
            "PropertiesChanged",
            32,
        )
        .await?;

        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(message) => {
                        let path_matches = message
                            .header()
                            .path()
                            .map(|path| path.as_str() == unit_path)
                            .unwrap_or(false);
                        if !path_matches {
                            continue;
                        }

                        let Ok((changed_interface, changed_properties, invalidated_properties)) =
                            message.body().deserialize::<(String, NmProperties, Vec<String>)>()
                        else {
                            continue;
                        };

                        if changed_interface != SYSTEMD_UNIT_IFACE {
                            continue;
                        }

                        let active_state_changed = changed_properties.contains_key("ActiveState")
                            || invalidated_properties
                                .iter()
                                .any(|property| property == "ActiveState");
                        if !active_state_changed {
                            continue;
                        }

                        let new_state = dbus::property::<String>(&changed_properties, "ActiveState")
                            .unwrap_or_else(|| "unknown".to_string());
                        let new_state = if new_state == "unknown"
                            && invalidated_properties.iter().any(|property| property == "ActiveState")
                        {
                            NetworkManagerService::networkmanager_systemd_active_state().await
                        } else {
                            new_state
                        };

                        NetworkManagerService::handle_networkmanager_systemd_state(&new_state).await;
                    }
                    Err(error) => {
                        error!("NetworkManager systemd PropertiesChanged stream error: {}", error);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn initialize_status_cache() -> Result<()> {
        Self::ensure_status_cache().await
    }

    pub fn initialize_status_cache_in_background() {
        if NETWORK_STATUS_INIT_STARTED.load(Ordering::Acquire) {
            return;
        }

        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };

        if NETWORK_STATUS_INIT_STARTED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        handle.spawn(async {
            if let Err(error) = Self::initialize_status_cache().await {
                NETWORK_STATUS_INIT_STARTED.store(false, Ordering::Release);
                error!("failed to initialize NetworkManager status cache: {}", error);
            }
        });
    }

    pub(super) async fn ensure_status_cache() -> Result<()> {
        NETWORK_STATUS_WATCHER
            .get_or_try_init(|| async {
                Self::spawn_systemd_status_watcher().await?;
                let current_state = Self::networkmanager_systemd_active_state().await;
                Self::handle_networkmanager_systemd_state(&current_state).await;
                Ok(())
            })
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/unit/services/network_manager_service/status/snapshot.rs"]
mod tests;
