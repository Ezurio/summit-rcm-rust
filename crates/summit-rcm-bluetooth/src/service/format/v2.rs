//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::super::*;
use crate::routes::shared::include_filter;
use crate::routes::v2::{BluetoothControllerModel, BluetoothStateResponse};

impl BluetoothService {
    pub(in crate::service) fn format_controller_state_v2(
        controller_name: String,
        snapshot: &BluetoothSnapshot,
        powered: bool,
        discoverable: bool,
        cached_filters: &ControllerFilterCache,
        matched_filters: Option<&[String]>,
    ) -> BluetoothStateResponse {
        let model = BluetoothControllerModel {
            bluetooth_devices: include_filter(matched_filters, "bluetoothDevices")
                .then(|| Self::bluetooth_device_models(snapshot)),
            rssi: include_filter(matched_filters, "RSSI")
                .then_some(cached_filters.rssi)
                .flatten(),
            transport: include_filter(matched_filters, "Transport")
                .then_some(cached_filters.transport.clone())
                .flatten(),
            pattern: include_filter(matched_filters, "Pattern")
                .then_some(cached_filters.pattern.clone())
                .flatten(),
            discovering: include_filter(matched_filters, "discovering")
                .then_some(i32::from(snapshot.discovering)),
            powered: include_filter(matched_filters, "powered").then_some(i32::from(powered)),
            discoverable: include_filter(matched_filters, "discoverable")
                .then_some(i32::from(discoverable)),
        };
        BluetoothStateResponse(BTreeMap::from([(controller_name, model)]))
    }
}