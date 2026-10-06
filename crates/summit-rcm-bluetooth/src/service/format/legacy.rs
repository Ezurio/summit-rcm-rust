//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::super::*;
use crate::routes::legacy::LegacyBluetoothControllerModel;
use crate::routes::shared::include_filter;

impl BluetoothService {
    pub(in crate::service) fn format_controller_state_legacy(
        controller_name: String,
        snapshot: &BluetoothSnapshot,
        powered: bool,
        discoverable: bool,
        cached_filters: &ControllerFilterCache,
        matched_filters: Option<&[String]>,
    ) -> BTreeMap<String, LegacyBluetoothControllerModel> {
        let model = LegacyBluetoothControllerModel {
            bluetooth_devices: include_filter(matched_filters, "bluetoothDevices")
                .then(|| Self::bluetooth_device_models(snapshot)),
            discovering: include_filter(matched_filters, "discovering")
                .then_some(i32::from(snapshot.discovering)),
            powered: include_filter(matched_filters, "powered").then_some(i32::from(powered)),
            discoverable: include_filter(matched_filters, "discoverable")
                .then_some(i32::from(discoverable)),
            transport_filter: include_filter(matched_filters, "transportFilter")
                .then_some(cached_filters.transport.clone())
                .flatten(),
        };
        BTreeMap::from([(controller_name, model)])
    }
}
