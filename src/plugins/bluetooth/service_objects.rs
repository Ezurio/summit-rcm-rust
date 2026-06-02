//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! BlueZ D-Bus object helpers for the Bluetooth service: connection handling,
//! managed-object queries, snapshot extraction, and notification emission.

use super::*;

impl BluetoothService {
    pub(super) async fn get_controller_state_data_with_conn(
        conn: &Connection,
        controller: Option<&str>,
    ) -> anyhow::Result<ControllerStateData> {
        let objects = Self::get_managed_objects(conn).await?;
        let snapshot = Self::snapshot_from_objects(&objects, controller, false)?;

        let adapter_path = Self::get_adapter_path(&objects, controller)
            .ok_or_else(|| anyhow::anyhow!("No Bluetooth adapter found"))?;

        let adapter_props = objects
            .iter()
            .find(|(path, _)| path.as_str() == adapter_path)
            .and_then(|(_, ifaces)| ifaces.get(ADAPTER_IFACE))
            .ok_or_else(|| anyhow::anyhow!("Adapter interface not found"))?;

        Ok(ControllerStateData {
            controller_name: Self::controller_name_for_path(&objects, &adapter_path),
            snapshot,
            powered: dbus::property_or_default(adapter_props, "Powered"),
            discoverable: dbus::property_or_default(adapter_props, "Discoverable"),
        })
    }

    pub(super) async fn call_bluez_noargs(
        conn: &Connection,
        path: &str,
        interface: &str,
        method: &str,
    ) -> anyhow::Result<()> {
        dbus::call_method(
            conn,
            Some(BLUEZ_SERVICE),
            path,
            Some(interface),
            method,
            &(),
            None,
        )
            .await?;
        Ok(())
    }

    pub(super) async fn set_bluez_bool_property(
        conn: &Connection,
        path: &str,
        interface: &str,
        property: &str,
        value: bool,
    ) -> anyhow::Result<()> {
        crate::dbus::set_property_with_timeout(
            conn,
            BLUEZ_SERVICE,
            path,
            interface,
            property,
            Value::from(value),
            None,
        )
        .await
    }

    pub(super) fn device_path(adapter_path: &str, device_address: &str) -> String {
        format!("{}/dev_{}", adapter_path, device_address.replace(':', "_"))
    }

    #[cfg(all(feature = "bluetooth-websocket", feature = "bluetooth-vsp"))]
    pub(super) fn find_device_path(objects: &ManagedObjects, device_address: &str) -> Option<String> {
        objects.iter().find_map(|(path, ifaces)| {
            let device_props = ifaces.get(DEVICE_IFACE)?;
            let address: String = dbus::property_or_default(device_props, "Address");
            if address.eq_ignore_ascii_case(device_address) {
                Some(path.as_str().to_string())
            } else {
                None
            }
        })
    }

    pub(super) async fn get_conn() -> anyhow::Result<Arc<Connection>> {
        dbus::system_bus().await
    }

    pub(super) fn snapshot_from_objects(
        objects: &ManagedObjects,
        controller: Option<&str>,
        include_gatt: bool,
    ) -> anyhow::Result<BluetoothSnapshot> {
        let adapter_path = Self::get_adapter_path(objects, controller)
            .ok_or_else(|| anyhow::anyhow!("No Bluetooth adapter found"))?;

        let adapter_props = objects
            .iter()
            .find(|(path, _)| path.as_str() == adapter_path)
            .and_then(|(_, ifaces)| ifaces.get(ADAPTER_IFACE))
            .ok_or_else(|| anyhow::anyhow!("Adapter interface not found"))?;

        let mut devices = BTreeMap::new();
        for (path, ifaces) in objects {
            if !ifaces.contains_key(DEVICE_IFACE) {
                continue;
            }
            if !path.as_str().starts_with(&adapter_path) {
                continue;
            }

            if let Some(dev_props) = ifaces.get(DEVICE_IFACE) {
                let mut device = Self::device_snapshot(dev_props);
                if include_gatt {
                    let (services, characteristics) =
                        Self::build_device_gatt_snapshot(objects, path.as_str());
                    device.services = services;
                    device.characteristics = characteristics;
                }
                let key = if device.address.is_empty() {
                    path.as_str().to_string()
                } else {
                    device.address.clone()
                };
                devices.insert(key, device);
            }
        }

        Ok(BluetoothSnapshot {
            discovering: dbus::property_or_default(adapter_props, "Discovering"),
            devices,
        })
    }

    pub(super) async fn get_managed_objects(conn: &Connection) -> anyhow::Result<ManagedObjects> {
        let reply = dbus::call_method(
            conn,
            Some(BLUEZ_SERVICE),
            "/",
            Some(OBJECT_MANAGER_IFACE),
            "GetManagedObjects",
            &(),
            None,
        )
        .await?;
        let objects: ManagedObjects = reply.body().deserialize()?;
        Ok(objects)
    }

    pub(super) fn get_adapter_path(objects: &ManagedObjects, controller: Option<&str>) -> Option<String> {
        let adapter_paths = Self::adapter_paths(objects);

        if let Some(ctrl) = controller {
            if let Some(index) = ctrl.strip_prefix("controller").and_then(|value| value.parse::<usize>().ok())
                && let Some(path) = adapter_paths.get(index) {
                    return Some(path.clone());
                }

            let want = format!("/org/bluez/{}", ctrl);
            if adapter_paths.iter().any(|path| path == &want) {
                return Some(want);
            }
        }

        adapter_paths.into_iter().next()
    }

    pub(super) fn timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    pub(super) fn device_snapshot(props: &HashMap<String, OwnedValue>) -> DeviceSnapshot {
        DeviceSnapshot {
            address: dbus::property_or_default(props, "Address"),
            name: dbus::property_or_default(props, "Name"),
            alias: dbus::property_or_default(props, "Alias"),
            class: dbus::property(props, "Class"),
            icon: dbus::property_or_default(props, "Icon"),
            rssi: dbus::property(props, "RSSI"),
            connected: dbus::property_or_default(props, "Connected"),
            paired: dbus::property_or_default(props, "Paired"),
            services: BTreeMap::new(),
            characteristics: BTreeMap::new(),
        }
    }

    pub(super) fn build_device_gatt_snapshot(
        objects: &ManagedObjects,
        device_path: &str,
    ) -> (BTreeMap<String, ServiceSnapshot>, BTreeMap<String, CharacteristicSnapshot>) {
        let mut service_paths: Vec<(String, String, String)> = Vec::with_capacity(objects.len());
        let mut services = BTreeMap::new();

        for (path, ifaces) in objects {
            if !path.as_str().starts_with(device_path) {
                continue;
            }

            let Some(service_props) = ifaces.get(GATT_SERVICE_IFACE) else {
                continue;
            };

            let service_uuid: String = dbus::property_or_default(service_props, "UUID");
            if service_uuid.is_empty() {
                continue;
            }

            let service_path = path.as_str().to_string();
            let service_uuid_lower = service_uuid.to_ascii_lowercase();
            service_paths.push((service_path, service_uuid.clone(), service_uuid_lower));
            services.entry(service_uuid).or_insert_with(ServiceSnapshot::default);
        }

        service_paths.sort_by(|left, right| left.0.cmp(&right.0));

        let mut characteristics = BTreeMap::new();
        for (service_path, service_uuid, service_uuid_lower) in &service_paths {
            for (path, ifaces) in objects {
                if !path.as_str().starts_with(service_path) {
                    continue;
                }

                let Some(char_props) = ifaces.get(GATT_CHR_IFACE) else {
                    continue;
                };

                let char_uuid: String = dbus::property_or_default(char_props, "UUID");
                if char_uuid.is_empty() {
                    continue;
                }

                let flags: Vec<String> = dbus::property_or_default(char_props, "Flags");
                services
                    .entry(service_uuid.clone())
                    .or_insert_with(ServiceSnapshot::default)
                    .characteristics
                    .insert(char_uuid.clone(), flags);

                let key = format!("{}::{}", service_uuid_lower, char_uuid.to_ascii_lowercase());
                characteristics.insert(
                    key,
                    CharacteristicSnapshot {
                        uuid: char_uuid,
                        value_hex: dbus::property_bytes_hex(char_props, "Value"),
                    },
                );
            }
        }

        (services, characteristics)
    }

    pub(super) fn find_characteristic_path(
        objects: &ManagedObjects,
        device_path: &str,
        service_uuid: &str,
        char_uuid: &str,
    ) -> Option<String> {
        let service_path = objects.iter().find_map(|(path, ifaces)| {
            let service_props = ifaces.get(GATT_SERVICE_IFACE)?;
            if !path.as_str().starts_with(device_path) {
                return None;
            }

            let current_uuid: String = dbus::property_or_default(service_props, "UUID");
            if current_uuid.eq_ignore_ascii_case(service_uuid) {
                Some(path.as_str().to_string())
            } else {
                None
            }
        })?;

        objects.iter().find_map(|(path, ifaces)| {
            let char_props = ifaces.get(GATT_CHR_IFACE)?;
            if !path.as_str().starts_with(&service_path) {
                return None;
            }

            let current_uuid: String = dbus::property_or_default(char_props, "UUID");
            if current_uuid.eq_ignore_ascii_case(char_uuid) {
                Some(path.as_str().to_string())
            } else {
                None
            }
        })
    }

    pub(super) fn send_char_value_notification(char_uuid: &str, value_hex: String) {
        send_notification(serde_json::json!({
            "char": {
                "char_uuid": char_uuid,
                "value": value_hex,
                "timestamp": Self::timestamp(),
            }
        }));
    }

    pub(super) fn send_char_result_notification(char_uuid: &str, result: i32, error: Option<String>) {
        send_notification(serde_json::json!({
            "char": {
                "char_uuid": char_uuid,
                "result": result,
                "error": error,
                "timestamp": Self::timestamp(),
            }
        }));
    }
}
