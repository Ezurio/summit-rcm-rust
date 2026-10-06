//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

// utoipa stays optional and is enabled only under `api-docs`. That keeps
// OpenAPI derives compile-gated: an ungated `utoipa::` use fails when
// `api-docs` is off, and this crate does not pull the dependency into
// non-doc builds.

// Bluetooth plugin routes and BLE notification websocket support.

pub mod routes;
pub mod service;

#[macro_export]
macro_rules! declare_bluetooth_api {
    (command: $command:expr, handler: $handler:path) => {
        inventory::submit! {
            summit_rcm_bluetooth::service::BluetoothCommandHandlerRegistration {
                command: $command,
                handle: $handler,
            }
        }
    };
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
summit_rcm_web::declare_web_api! {
    name: "bluetooth",
    routes {
        v2 => [
            protected SomeProvisioning "/api/v2/bluetooth" => {
                GET => routes::v2::get_bluetooth,
                PUT => routes::v2::put_bluetooth
            },
            protected SomeProvisioning "/api/v2/bluetooth/{controller}" => {
                GET => routes::v2::get_bluetooth_controller,
                PUT => routes::v2::put_bluetooth_controller
            },
            protected SomeProvisioning "/api/v2/bluetooth/{controller}/{device}" => {
                GET => routes::v2::get_bluetooth_device,
                PUT => routes::v2::put_bluetooth_device
            },
            #[cfg(feature = "bluetooth-websocket")]
            protected SomeProvisioning "/api/v2/bluetooth/ws" => {
                GET => routes::v2::get_bluetooth_websocket
            },
        ],
        legacy => [
            protected SomeProvisioning "/bluetooth" => {
                GET => routes::legacy::get_bluetooth_legacy,
                PUT => routes::legacy::put_bluetooth_legacy
            },
            protected SomeProvisioning "/bluetooth/{controller}" => {
                GET => routes::legacy::get_bluetooth_controller_legacy,
                PUT => routes::legacy::put_bluetooth_controller_legacy
            },
            protected SomeProvisioning "/bluetooth/{controller}/{device}" => {
                GET => routes::legacy::get_bluetooth_device_legacy,
                PUT => routes::legacy::put_bluetooth_device_legacy
            },
            #[cfg(feature = "bluetooth-websocket")]
            protected SomeProvisioning "/bluetoothWebsocket" => {
                GET => routes::legacy::get_bluetooth_websocket_index_legacy
            },
            #[cfg(feature = "bluetooth-websocket")]
            protected SomeProvisioning "/bluetoothWebsocket/" => {
                GET => routes::legacy::get_bluetooth_websocket_index_legacy
            },
            #[cfg(feature = "bluetooth-websocket")]
            protected SomeProvisioning "/bluetoothWebsocket/ws" => {
                GET => routes::legacy::get_bluetooth_websocket_legacy
            },
        ],
    },
}
