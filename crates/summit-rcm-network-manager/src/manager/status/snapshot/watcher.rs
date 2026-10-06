//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Cache lifecycle and signal watchers that keep the NetworkManager status cache
//! up to date in response to D-Bus PropertiesChanged and systemd state changes.

use anyhow::Result;
use log::error;
use serde_json::json;
use std::sync::atomic::Ordering;
use summit_rcm_core::dbus;
use summit_rcm_core::dbus::DBUS_PROP_IFACE;
use summit_rcm_core::systemd_unit::SystemdUnit;
use summit_rcm_core::systemd_unit::{SYSTEMD_BUS_NAME, SYSTEMD_UNIT_IFACE};

use super::super::super::{
    NETWORK_STATUS_CACHE, NETWORK_STATUS_INIT_STARTED, NETWORK_STATUS_SIGNAL_TASK,
    NETWORK_STATUS_WATCHER, NM_ACCESS_POINT_IFACE, NM_BUS_NAME, NM_DEVICE_IFACE,
    NM_DEVICE_WIRED_IFACE, NM_DEVICE_WIRELESS_IFACE, NM_IFACE, NM_MAIN_OBJ,
    NM_SETTINGS_CONNECTION_IFACE, NM_SETTINGS_IFACE, NetworkManagerService, NmProperties,
};

const NETWORKMANAGER_SERVICE_FILE: &str = "NetworkManager.service";

impl NetworkManagerService {
    async fn clear_status_cache() {
        *NETWORK_STATUS_CACHE.write().await = json!({});
    }

    pub(crate) fn update_cached_device_properties(
        cache: &mut serde_json::Value,
        object_path: &str,
        interface: &str,
        changed_properties: &NmProperties,
    ) -> bool {
        let Some(devices) = cache.as_object_mut() else {
            return false;
        };

        let mut matched = false;
        for raw_device_value in devices.values_mut() {
            let Some(raw_device) = raw_device_value.as_object_mut() else {
                continue;
            };

            let is_match = raw_device.get("path").and_then(serde_json::Value::as_str)
                == Some(object_path)
                || raw_device
                    .get("status")
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|status| {
                        status.values().any(|val| val.as_str() == Some(object_path))
                    })
                || raw_device
                    .get("wireless")
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|wireless| {
                        wireless
                            .get("ActiveAccessPoint")
                            .and_then(serde_json::Value::as_str)
                            == Some(object_path)
                    });

            if !is_match {
                continue;
            }

            matched = true;

            // Route properties directly to the appropriate sub-map based on the D-Bus interface
            let submap = match interface {
                NM_DEVICE_IFACE => raw_device
                    .get_mut("status")
                    .and_then(serde_json::Value::as_object_mut),
                NM_DEVICE_WIRED_IFACE => raw_device
                    .get_mut("wired")
                    .and_then(serde_json::Value::as_object_mut),
                NM_DEVICE_WIRELESS_IFACE => raw_device
                    .get_mut("wireless")
                    .and_then(serde_json::Value::as_object_mut),
                NM_ACCESS_POINT_IFACE => raw_device
                    .get_mut("ActiveAccessPoint")
                    .and_then(serde_json::Value::as_object_mut),
                _ => {
                    // For interface types like "org.freedesktop.NetworkManager.IP4Config",
                    // the submap key matches the interface's trailing component (e.g. "IP4Config" -> "Ip4Config")
                    // Look for an existing submap key that matches case-insensitively.
                    let trailing = interface.rsplit('.').next().unwrap_or(interface);
                    raw_device
                        .iter_mut()
                        .find(|(k, v)| v.is_object() && k.eq_ignore_ascii_case(trailing))
                        .and_then(|(_, v)| v.as_object_mut())
                }
            };

            let Some(submap) = submap else {
                continue;
            };

            for (key, owned_value) in changed_properties {
                let _ = submap.insert(key.clone(), dbus::owned_value_to_json(owned_value));
            }

            if interface == NM_DEVICE_IFACE {
                let Some(interface_name) = raw_device
                    .get("interface")
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let interface_name = interface_name.to_string();
                let details =
                    Self::interface_detail_fields(&interface_name, object_path, raw_device);
                raw_device.extend(details);
            }
        }

        matched
    }

    async fn handle_dbus_properties_changed(
        path: &str,
        interface: &str,
        changed: &NmProperties,
        invalidated: &[String],
    ) {
        if !path.starts_with(NM_MAIN_OBJ) {
            return;
        }

        if interface == NM_IFACE && path == NM_MAIN_OBJ {
            let devices_changed =
                changed.contains_key("Devices") || invalidated.iter().any(|prop| prop == "Devices");

            if devices_changed && let Err(error) = Self::refresh_status_cache().await {
                error!(
                    "failed to refresh status cache on NetworkManager topology change: {}",
                    error
                );
            }
            return;
        }

        if interface == NM_SETTINGS_CONNECTION_IFACE
            || (interface == NM_SETTINGS_IFACE
                && (changed.contains_key("Connections")
                    || invalidated.iter().any(|p| p == "Connections")))
        {
            if let Err(error) = Self::refresh_status_cache().await {
                error!(
                    "failed to refresh status cache on connection settings change: {}",
                    error
                );
            }
            return;
        }

        // Check if an object path pointer changed (e.g. ActiveConnection, Ip4Config, or ActiveAccessPoint changed to a different path).
        let has_pointer_change = changed.keys().any(|key| {
            matches!(
                key.as_str(),
                "ActiveConnection"
                    | "Ip4Config"
                    | "Ip6Config"
                    | "Dhcp4Config"
                    | "Dhcp6Config"
                    | "ActiveAccessPoint"
            )
        });

        if has_pointer_change {
            let target_path = {
                let cache = NETWORK_STATUS_CACHE.read().await;
                cache.as_object().and_then(|devices| {
                    devices.values().find_map(|dev| {
                        let obj = dev.as_object()?;
                        let is_match = obj.get("path").and_then(serde_json::Value::as_str)
                            == Some(path)
                            || obj
                                .get("status")
                                .and_then(serde_json::Value::as_object)
                                .is_some_and(|status| {
                                    status.values().any(|val| val.as_str() == Some(path))
                                });
                        if is_match {
                            obj.get("path")
                                .and_then(serde_json::Value::as_str)
                                .map(ToString::to_string)
                        } else {
                            None
                        }
                    })
                })
            };

            if let Some(dev_path) = target_path {
                if let Err(error) = Self::refresh_device_status_cache(&dev_path).await {
                    error!(
                        "failed to refresh device status cache for pointer change {}: {}",
                        path, error
                    );
                }
                return;
            }
        }

        let updated = {
            let mut cache = NETWORK_STATUS_CACHE.write().await;
            Self::update_cached_device_properties(&mut cache, path, interface, changed)
        };

        if !updated
            && interface == NM_DEVICE_IFACE
            && let Err(error) = Self::refresh_device_status_cache(path).await
        {
            error!(
                "failed to refresh device status cache for new device {}: {}",
                path, error
            );
        }
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

                NetworkManagerService::handle_dbus_properties_changed(
                    &signal.path,
                    &signal.interface,
                    &signal.changed,
                    &signal.invalidated,
                )
                .await;
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
                    error!(
                        "failed to refresh NetworkManager status cache on service activation: {}",
                        error
                    );
                }
                if let Err(error) = Self::ensure_nm_status_watcher().await {
                    error!(
                        "failed to start NetworkManager status watcher on service activation: {}",
                        error
                    );
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
                error!(
                    "failed to initialize NetworkManager status cache: {}",
                    error
                );
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
