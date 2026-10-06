//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::super::*;

impl BluetoothService {
    pub(in crate::service) fn bluetooth_device_models(
        snapshot: &BluetoothSnapshot,
    ) -> Vec<BluetoothDeviceModel> {
        let mut devices = Vec::with_capacity(snapshot.devices.len());
        for device in snapshot.devices.values() {
            devices.push(Self::device_model_from_snapshot(device));
        }
        devices
    }

    pub(in crate::service) fn device_model_from_snapshot(
        device: &DeviceSnapshot,
    ) -> BluetoothDeviceModel {
        super::raw::RawBluetoothDeviceView::from(device).into()
    }
}
