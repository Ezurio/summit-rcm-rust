 //
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Bluetooth control-command pipeline: the typed request entry points, the
//! command dispatcher, and the device-targeted PUT handlers (pairing,
//! connection, trust, removal) plus the GATT read helper. Split out of
//! `service.rs` to keep that module focused on service setup and state reads.

use super::*;
use summit_rcm_web::serde_json;

impl BluetoothService {
    /// Apply a device-targeted PUT, mirroring the Python `set_device_properties`
    /// order: settable properties, then `autoConnect`, `paired`, `connected`,
    /// and finally `passkey`. `paired = 0` removes the device and returns early.
    async fn apply_device_properties(
        conn: &Connection,
        adapter_path: &str,
        device_address: &str,
        body: &BluetoothCommandRequest,
    ) -> anyhow::Result<BluetoothCommandOutcome> {
        let device_path = Self::device_path(adapter_path, device_address);

        // Directly settable boolean device properties.
        for (name, value) in [
            ("Trusted", body.trusted),
            ("AutoConnect", body.auto_connect),
            ("AutoConnectAutoDisable", body.auto_connect_auto_disable),
        ] {
            if let Some(value) = value {
                Self::set_bluez_bool_property(conn, &device_path, DEVICE_IFACE, name, value != 0)
                    .await?;
            }
        }

        if body.auto_connect == Some(1) {
            agent::ensure_agent_registered(conn).await;
        }

        if let Some(paired) = body.paired {
            if paired == 1 {
                let already_paired: bool =
                    dbus::get_property(conn, BLUEZ_SERVICE, &device_path, DEVICE_IFACE, "Paired")
                        .await
                        .unwrap_or(false);
                if !already_paired {
                    agent::ensure_agent_registered(conn).await;
                    Self::call_bluez_noargs(conn, &device_path, DEVICE_IFACE, "Pair").await?;
                }
            } else if paired == 0 {
                Self::remove_device(conn, adapter_path, &device_path).await?;
                return Ok(BluetoothCommandOutcome::success(Self::empty_control_response()));
            }
        }

        if let Some(connected) = body.connected {
            let connected_state: bool =
                dbus::get_property(conn, BLUEZ_SERVICE, &device_path, DEVICE_IFACE, "Connected")
                    .await
                    .unwrap_or(false);
            if i32::from(connected_state) != connected {
                if connected == 1 {
                    agent::ensure_agent_registered(conn).await;
                    Self::call_bluez_noargs(conn, &device_path, DEVICE_IFACE, "Connect").await?;
                } else if connected == 0 {
                    Self::call_bluez_noargs(conn, &device_path, DEVICE_IFACE, "Disconnect").await?;
                }
            }
        }

        if let Some(passkey) = body.passkey.as_deref()
            && let Ok(passkey) = passkey.parse::<u32>()
        {
            agent::set_passkey(&device_path, passkey);
        }

        Self::refresh_managed_object_cache(conn).await?;

        Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
    }

    /// Disconnect (if connected) and remove a device from its adapter, mirroring
    /// the Python `remove_device_method`.
    async fn remove_device(
        conn: &Connection,
        adapter_path: &str,
        device_path: &str,
    ) -> anyhow::Result<()> {
        let connected: bool =
            dbus::get_property(conn, BLUEZ_SERVICE, device_path, DEVICE_IFACE, "Connected")
                .await
                .unwrap_or(false);
        if connected {
            Self::call_bluez_noargs(conn, device_path, DEVICE_IFACE, "Disconnect").await?;
        }

        let device_object = OwnedObjectPath::try_from(device_path)
            .map_err(|error| anyhow::anyhow!("invalid device path {device_path}: {error}"))?;
        let _ = dbus::call_method(
            conn,
            Some(BLUEZ_SERVICE),
            adapter_path,
            Some(ADAPTER_IFACE),
            "RemoveDevice",
            &(device_object,),
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn handle_command_legacy(
        controller: Option<&str>,
        device: Option<&str>,
        body: BluetoothCommandRequest,
    ) -> anyhow::Result<(BluetoothControlResponse, bool, String)> {
        Self::run_command(controller, device, &body).await
    }

    /// Run a Bluetooth control command from an already-encoded JSON body.
    ///
    /// Device route handlers (e.g. the VSP/HID sub-resources) build their own
    /// request as JSON because it carries device-specific parameters. It is
    /// parsed back into the shared [`BluetoothCommandRequest`] once here (the
    /// device params land in its flattened `extra`) and then runs through the
    /// same typed pipeline as the core routes.
    pub async fn run_command_value(
        controller: Option<&str>,
        device: Option<&str>,
        body: serde_json::Value,
    ) -> anyhow::Result<(BluetoothControlResponse, bool, String)> {
        let request: BluetoothCommandRequest = serde_json::from_value(body)?;
        Self::run_command(controller, device, &request).await
    }

    pub async fn run_command_value_v2(
        controller: Option<&str>,
        device: Option<&str>,
        body: serde_json::Value,
    ) -> anyhow::Result<BluetoothCommandV2Result> {
        let request: BluetoothCommandRequest = serde_json::from_value(body)?;
        Self::run_command_v2(controller, device, &request).await
    }

    async fn run_command(
        controller: Option<&str>,
        device: Option<&str>,
        body: &BluetoothCommandRequest,
    ) -> anyhow::Result<(BluetoothControlResponse, bool, String)> {
        match Self::handle_command_inner(controller, device, body).await {
            Ok(outcome) => Ok((outcome.response, outcome.succeeded, outcome.info_msg)),
            Err(error) => {
                error!("bluetooth handle_command error: {}", error);
                let info_msg = error
                    .downcast_ref::<BluetoothCommandRouteError>()
                    .map(BluetoothCommandRouteError::legacy_info_msg)
                    .unwrap_or_else(|| error.to_string());
                let outcome = BluetoothCommandOutcome::failure(info_msg);
                Ok((outcome.response, outcome.succeeded, outcome.info_msg))
            }
        }
    }

    pub async fn handle_command_v2(
        controller: Option<&str>,
        device: Option<&str>,
        body: BluetoothCommandRequest,
    ) -> anyhow::Result<BluetoothCommandV2Result> {
        Self::run_command_v2(controller, device, &body).await
    }

    async fn run_command_v2(
        controller: Option<&str>,
        device: Option<&str>,
        body: &BluetoothCommandRequest,
    ) -> anyhow::Result<BluetoothCommandV2Result> {
        match Self::handle_command_inner(controller, device, body).await {
            Ok(outcome) => Ok(Ok((outcome.response, outcome.info_msg))),
            Err(error) => match error.downcast::<BluetoothCommandRouteError>() {
                Ok(route_error) => Ok(Err(route_error)),
                Err(error) => {
                    error!("bluetooth handle_command_v2 error: {}", error);
                    Err(error)
                }
            },
        }
    }

    /// Issue a GATT `ReadValue` D-Bus call for a characteristic, returning the
    /// raw reply.
    async fn read_characteristic(conn: &Connection, char_path: &str) -> anyhow::Result<zbus::Message> {
        dbus::call_method(
            conn,
            Some(BLUEZ_SERVICE),
            char_path,
            Some(GATT_CHR_IFACE),
            "ReadValue",
            &HashMap::<String, OwnedValue>::new(),
            None,
        )
        .await
    }

    /// Read a characteristic and publish its value as a `char` notification.
    #[cfg(feature = "bluetooth-websocket")]
    async fn read_characteristic_and_notify(
        conn: &Connection,
        char_path: &str,
        char_uuid: &str,
    ) -> anyhow::Result<()> {
        let reply = Self::read_characteristic(conn, char_path).await?;
        Self::send_char_value_notification(char_uuid, hex::encode(reply.body().deserialize::<Vec<u8>>()?));
        Ok(())
    }

    /// Without the websocket feature there is no notification sink, so the read
    /// is still issued (for its device-side effect) and the value is dropped.
    #[cfg(not(feature = "bluetooth-websocket"))]
    async fn read_characteristic_and_notify(
        conn: &Connection,
        char_path: &str,
        _char_uuid: &str,
    ) -> anyhow::Result<()> {
        let _ = Self::read_characteristic(conn, char_path).await?;
        Ok(())
    }

    pub(super) async fn handle_command_inner(
        controller: Option<&str>,
        device: Option<&str>,
        body: &BluetoothCommandRequest,
    ) -> anyhow::Result<BluetoothCommandOutcome> {
        let conn = Self::get_conn().await?;
        let conn = conn.as_ref();
        let objects = Self::get_managed_objects(conn).await?;

        let adapter_path = Self::get_adapter_path(&objects, controller)
            .ok_or_else(|| anyhow::Error::new(BluetoothCommandRouteError::NoBluetoothAdapter))?;

        let command = body.command.as_deref().unwrap_or("");

        for handler in BLUETOOTH_COMMAND_HANDLERS.iter() {
            if handler.command == command {
                // The device-handler boundary is the one intentionally
                // type-erased seam: each device plugin parses its own params out
                // of this JSON body.
                let forwarded = Self::encode_command_request(body)?;
                return (handler.handle)(BluetoothCommandContext {
                    conn,
                    objects: &objects,
                    adapter_path: adapter_path.as_str(),
                    device,
                    body: &forwarded,
                    command,
                })
                .await;
            }
        }

        match command {
            "" => {
                // A device-targeted PUT sets device properties (pairing,
                // connection, trust, …); an adapter-targeted PUT sets adapter
                // properties (power, discovery filters, discoverable). This
                // mirrors the Python REST split between `set_device_properties`
                // and `set_adapter_properties`.
                if let Some(dev_addr) = device {
                    return Self::apply_device_properties(conn, &adapter_path, dev_addr, body).await;
                }

                let powering_off = body.powered.is_some_and(|powered| powered == 0);

                if let Some(powered) = body.powered {
                    Self::set_bluez_bool_property(
                        conn,
                        adapter_path.as_str(),
                        ADAPTER_IFACE,
                        "Powered",
                        powered != 0,
                    )
                    .await?;
                }

                // BlueZ rejects discovery-filter and discoverable changes while
                // powering off, so skip them in that case (matches Python).
                if !powering_off {
                    let controller_name =
                        Self::controller_name_for_path(&objects, &adapter_path);
                    Self::apply_discovery_filters(
                        conn,
                        adapter_path.as_str(),
                        &controller_name,
                        body,
                    )
                    .await?;
                    if let Some(disc) = body.discoverable {
                        Self::set_bluez_bool_property(
                            conn,
                            adapter_path.as_str(),
                            ADAPTER_IFACE,
                            "Discoverable",
                            disc != 0,
                        )
                        .await?;
                    }
                }
                Self::refresh_managed_object_cache(conn).await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleStartDiscovery" => {
                Self::call_bluez_noargs(conn, adapter_path.as_str(), ADAPTER_IFACE, "StartDiscovery")
                    .await?;
                Self::refresh_managed_object_cache(conn).await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleStopDiscovery" => {
                Self::call_bluez_noargs(conn, adapter_path.as_str(), ADAPTER_IFACE, "StopDiscovery")
                    .await?;
                Self::refresh_managed_object_cache(conn).await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            #[cfg(feature = "bluetooth-websocket")]
            "bleEnableWebsockets" => {
                Self::enable_websocket_notifications().await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleGatt" => {
                let dev_addr = device
                    .ok_or_else(|| anyhow::anyhow!("device address not specified"))?;
                let service_uuid = body
                    .svc_uuid
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("svcUuid param not specified"))?;
                let char_uuid = body
                    .chr_uuid
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("charUuid param not specified"))?;
                let operation = body
                    .operation
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("operation param not specified"))?;
                let device_path = Self::device_path(&adapter_path, dev_addr);
                let char_path = Self::find_characteristic_path(
                    &objects,
                    &device_path,
                    service_uuid,
                    char_uuid,
                )
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Characteristic UUID {} not found for service {} and device {}",
                        char_uuid,
                        service_uuid,
                        dev_addr,
                    )
                })?;

                match operation {
                    BluetoothGattOperation::Read => {
                        Self::read_characteristic_and_notify(conn, char_path.as_str(), char_uuid)
                            .await?;
                    }
                    BluetoothGattOperation::Write => {
                        let value = body
                            .value
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("value param not specified"))?;
                        let value_bytes = hex::decode(value)?;
                        match dbus::call_method(
                            conn,
                            Some(BLUEZ_SERVICE),
                            char_path.as_str(),
                            Some(GATT_CHR_IFACE),
                            "WriteValue",
                            &(value_bytes, HashMap::<String, OwnedValue>::new()),
                            None,
                        )
                        .await
                        {
                            Ok(_) => {
                                #[cfg(feature = "bluetooth-websocket")]
                                Self::send_char_result_notification(char_uuid, 0, None);
                            }
                            Err(error) => {
                                #[cfg(feature = "bluetooth-websocket")]
                                Self::send_char_result_notification(
                                    char_uuid,
                                    -1,
                                    Some(error.to_string()),
                                );
                                return Err(error);
                            }
                        }
                    }
                    BluetoothGattOperation::Notify => {
                        let enable = body
                            .enable
                            .ok_or_else(|| anyhow::anyhow!("enable param not specified"))?;
                        let method = if enable { "StartNotify" } else { "StopNotify" };
                        Self::call_bluez_noargs(conn, char_path.as_str(), GATT_CHR_IFACE, method)
                            .await?;
                    }
                }
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleConnect" => {
                if let Some(dev_addr) = device {
                    let dev_path = Self::device_path(&adapter_path, dev_addr);
                    Self::call_bluez_noargs(conn, dev_path.as_str(), DEVICE_IFACE, "Connect")
                        .await?;
                }
                Self::refresh_managed_object_cache(conn).await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "bleDisconnect" => {
                if let Some(dev_addr) = device {
                    let dev_path = Self::device_path(&adapter_path, dev_addr);
                    Self::call_bluez_noargs(conn, dev_path.as_str(), DEVICE_IFACE, "Disconnect")
                        .await?;
                }
                Self::refresh_managed_object_cache(conn).await?;
                Ok(BluetoothCommandOutcome::success(Self::empty_control_response()))
            }
            "getConnInfo" => {
                let dev_addr = device
                    .ok_or_else(|| anyhow::anyhow!("Device not found"))?;
                let dev_path = Self::device_path(&adapter_path, dev_addr);
                let connected: bool = dbus::get_property(conn, BLUEZ_SERVICE, &dev_path, DEVICE_IFACE, "Connected")
                    .await
                    .unwrap_or(false);
                if !connected {
                    return Ok(BluetoothCommandOutcome::failure("Device not connected"));
                }

                match dbus::call_method(
                    conn,
                    Some(BLUEZ_SERVICE),
                    dev_path.as_str(),
                    Some(DEVICE_IFACE),
                    "GetConnInfo",
                    &(),
                    None,
                )
                .await
                {
                    Ok(reply) => {
                        let (rssi, tx_power, max_tx_power): (i16, i16, i16) =
                            reply.body().deserialize()?;
                        let mut response = Self::empty_control_response();
                        response.rssi = Some(i32::from(rssi));
                        response.tx_power = Some(i32::from(tx_power));
                        response.max_tx_power = Some(i32::from(max_tx_power));
                        Ok(BluetoothCommandOutcome::success(response))
                    }
                    Err(_) => Ok(BluetoothCommandOutcome::failure("Unable to get connection info")),
                }
            }
            _ => Err(anyhow::Error::new(BluetoothCommandRouteError::UnknownCommand(
                command.to_string(),
            ))),
        }
    }
}
