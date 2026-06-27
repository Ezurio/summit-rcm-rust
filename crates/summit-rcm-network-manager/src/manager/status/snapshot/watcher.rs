//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Cache lifecycle and signal watchers that keep the NetworkManager status cache
//! up to date in response to D-Bus PropertiesChanged and systemd state changes.

use anyhow::Result;
use summit_rcm_core::dbus;
use summit_rcm_core::dbus::DBUS_PROP_IFACE;
use summit_rcm_core::systemd_unit::SystemdUnit;
use summit_rcm_core::systemd_unit::{SYSTEMD_BUS_NAME, SYSTEMD_UNIT_IFACE};
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
        {
            let watcher = NETWORK_STATUS_SIGNAL_TASK.lock().await;
            if watcher.as_ref().is_some_and(|task| !task.is_finished()) {
                return Ok(());
            }
        }

        let handle = dbus::spawn_signal_observer(
            NM_BUS_NAME,
            DBUS_PROP_IFACE,
            "PropertiesChanged",
            64,
            |message| async move {
                let Some(signal) = dbus::parse_properties_changed(&message) else {
                    return;
                };

                if !NetworkManagerService::should_refresh_status_cache(
                    &signal.path,
                    &signal.interface,
                    &signal.changed,
                    &signal.invalidated,
                ) {
                    return;
                }

                if let Err(error) = NetworkManagerService::refresh_status_cache().await {
                    error!("failed to refresh cached NetworkManager status: {}", error);
                }
            },
        )
        .await?;

        let mut watcher = NETWORK_STATUS_SIGNAL_TASK.lock().await;
        if watcher.as_ref().is_some_and(|task| !task.is_finished()) {
            handle.abort();
        } else {
            if let Some(previous) = watcher.take() {
                previous.abort();
            }
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

        let task = dbus::spawn_signal_observer(
            SYSTEMD_BUS_NAME,
            DBUS_PROP_IFACE,
            "PropertiesChanged",
            32,
            move |message| {
                let unit_path = unit_path.clone();
                async move {
                    let Some(signal) = dbus::parse_properties_changed(&message) else {
                        return;
                    };

                    if signal.path != unit_path || signal.interface != SYSTEMD_UNIT_IFACE {
                        return;
                    }

                    let active_state_changed = signal.changed.contains_key("ActiveState")
                        || signal
                            .invalidated
                            .iter()
                            .any(|property| property == "ActiveState");
                    if !active_state_changed {
                        return;
                    }

                    let new_state = dbus::property::<String>(&signal.changed, "ActiveState")
                        .unwrap_or_else(|| "unknown".to_string());
                    let new_state = if new_state == "unknown"
                        && signal
                            .invalidated
                            .iter()
                            .any(|property| property == "ActiveState")
                    {
                        NetworkManagerService::networkmanager_systemd_active_state().await
                    } else {
                        new_state
                    };

                    NetworkManagerService::handle_networkmanager_systemd_state(&new_state).await;
                }
            },
        )
        .await?;
        drop(task);

        Ok(())
    }

    pub(crate) async fn initialize_status_cache() -> Result<()> {
        Self::ensure_status_cache().await
    }

    pub(crate) fn initialize_status_cache_in_background() {
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

        let init_task = handle.spawn(async {
            if let Err(error) = Self::initialize_status_cache().await {
                NETWORK_STATUS_INIT_STARTED.store(false, Ordering::Release);
                error!("failed to initialize NetworkManager status cache: {}", error);
            }
        });
        drop(init_task);
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
#[path = "../../../../tests/manager/status/snapshot/watcher/tests.rs"]
mod tests;
