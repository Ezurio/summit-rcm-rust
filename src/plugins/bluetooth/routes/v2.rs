//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::bluetooth::routes::shared::{
    include_filter, BluetoothCommandRequest, BluetoothControlResponse, BluetoothControllerState,
    BluetoothDeviceModel, BluetoothQuery,
};
use crate::plugins::bluetooth::service::{BluetoothDeviceStateError, BluetoothService};
use axum::{extract::{Path, Query}, Json};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct BluetoothControllerModel {
    #[serde(rename = "bluetoothDevices")]
    pub bluetooth_devices: Option<Vec<BluetoothDeviceModel>>,
    #[serde(rename = "RSSI")]
    pub rssi: Option<i32>,
    #[serde(rename = "Transport")]
    pub transport: Option<String>,
    #[serde(rename = "Pattern")]
    pub pattern: Option<String>,
    pub discovering: Option<i32>,
    pub powered: Option<i32>,
    pub discoverable: Option<i32>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct BluetoothStateResponse(pub BTreeMap<String, BluetoothControllerModel>);

impl BluetoothControllerState {
    pub fn into_v2_response(self) -> BluetoothStateResponse {
        let filters = self.matched_filters.as_deref();
        let model = BluetoothControllerModel {
            bluetooth_devices: include_filter(filters, "bluetoothDevices").then_some(self.devices),
            // Cached discovery-filter properties (write-only in BlueZ). The v2 API
            // exposes them under their BlueZ names.
            rssi: include_filter(filters, "RSSI").then_some(self.rssi_filter).flatten(),
            transport: include_filter(filters, "Transport").then_some(self.transport_filter).flatten(),
            pattern: include_filter(filters, "Pattern").then_some(self.pattern_filter).flatten(),
            discovering: include_filter(filters, "discovering").then_some(i32::from(self.discovering)),
            powered: include_filter(filters, "powered").then_some(i32::from(self.powered)),
            discoverable: include_filter(filters, "discoverable").then_some(i32::from(self.discoverable)),
        };
        BluetoothStateResponse(BTreeMap::from([(self.controller_name, model)]))
    }
}

define_bluetooth_v2_response_family! {
    pub enum GetBluetoothResponses(BluetoothStateResponse);
}

define_bluetooth_v2_response_family! {
    pub enum PutBluetoothResponses(BluetoothControlResponse);
}

define_bluetooth_v2_response_family! {
    pub enum GetBluetoothDeviceResponses(BluetoothDeviceModel);
}

fn classify_get_error<R>(error: &anyhow::Error, not_found: R, bad_request: R, internal_error: R) -> R {
    let message = error.to_string();
    if message.contains("controller not found") {
        not_found
    } else if message.contains("device not found") || message.contains("filters") {
        bad_request
    } else {
        internal_error
    }
}

fn get_error_response(error: &anyhow::Error) -> GetBluetoothResponses {
    classify_get_error(
        error,
        GetBluetoothResponses::NotFound,
        GetBluetoothResponses::BadRequest,
        GetBluetoothResponses::InternalError,
    )
}

fn put_error_response(message: &str) -> PutBluetoothResponses {
    if message.contains("command") && message.contains("invalid") {
        PutBluetoothResponses::BadRequest
    } else if message.contains("No Bluetooth adapter found") || message.contains("controller not found") {
        PutBluetoothResponses::NotFound
    } else {
        PutBluetoothResponses::InternalError
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/bluetooth",
    tag = "bluetooth",
    params(("filter" = Option<String>, Query, description = "Comma-separated response field filters")),
    responses(GetBluetoothResponses)
))]
pub async fn get_bluetooth(Query(query): Query<BluetoothQuery>) -> GetBluetoothResponses {
    match BluetoothService::get_controller_state(None, query.filters()).await {
        Ok(state) => state.into_v2_response().into(),
        Err(error) => get_error_response(&error),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/bluetooth",
    tag = "bluetooth",
    request_body = BluetoothCommandRequest,
    responses(PutBluetoothResponses)
))]
pub async fn put_bluetooth(Json(body): Json<BluetoothCommandRequest>) -> PutBluetoothResponses {
    match BluetoothService::handle_command_v2(None, None, body).await {
        Ok((value, info_msg)) => {
            if info_msg.contains("No Bluetooth adapter found") || info_msg.contains("controller not found") {
                PutBluetoothResponses::NotFound
            } else if info_msg.contains("Unknown command:") {
                PutBluetoothResponses::BadRequest
            } else {
                value.into()
            }
        }
        Err(error) => {
            log::error!("put_bluetooth invalid request or response shape: {}", error);
            PutBluetoothResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/bluetooth/{controller}",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("filter" = Option<String>, Query, description = "Comma-separated response field filters")
    ),
    responses(GetBluetoothResponses)
))]
pub async fn get_bluetooth_controller(
    Path(controller): Path<String>,
    Query(query): Query<BluetoothQuery>,
) -> GetBluetoothResponses {
    match BluetoothService::get_controller_state(Some(&controller), query.filters()).await {
        Ok(state) => state.into_v2_response().into(),
        Err(error) => get_error_response(&error),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/bluetooth/{controller}",
    tag = "bluetooth",
    params(("controller" = String, Path, description = "Controller address")),
    request_body = BluetoothCommandRequest,
    responses(PutBluetoothResponses)
))]
pub async fn put_bluetooth_controller(
    Path(controller): Path<String>,
    Json(body): Json<BluetoothCommandRequest>,
) -> PutBluetoothResponses {
    match BluetoothService::handle_command_v2(Some(&controller), None, body).await {
        Ok((value, info_msg)) => {
            if info_msg.contains("No Bluetooth adapter found") || info_msg.contains("controller not found") {
                put_error_response(&info_msg)
            } else {
                value.into()
            }
        }
        Err(error) => {
            log::error!("put_bluetooth_controller {} invalid request or response shape: {}", controller, error);
            PutBluetoothResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/bluetooth/{controller}/{device}",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("device" = String, Path, description = "Device address")
    ),
    responses(GetBluetoothDeviceResponses)
))]
pub async fn get_bluetooth_device(
    Path((controller, device)): Path<(String, String)>,
) -> GetBluetoothDeviceResponses {
    match BluetoothService::get_device_state_typed(&controller, &device).await {
        Ok(value) => value.into(),
        Err(BluetoothDeviceStateError::ControllerNotFound | BluetoothDeviceStateError::DeviceNotFound) => {
            GetBluetoothDeviceResponses::NotFound
        }
        Err(BluetoothDeviceStateError::Internal) => GetBluetoothDeviceResponses::InternalError,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/bluetooth/{controller}/{device}",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("device" = String, Path, description = "Device address")
    ),
    request_body = BluetoothCommandRequest,
    responses(PutBluetoothResponses)
))]
pub async fn put_bluetooth_device(
    Path((controller, device)): Path<(String, String)>,
    Json(body): Json<BluetoothCommandRequest>,
) -> PutBluetoothResponses {
    match BluetoothService::handle_command_v2(Some(&controller), Some(&device), body).await {
        Ok((value, info_msg)) => {
            if info_msg.contains("No Bluetooth adapter found") || info_msg.contains("controller not found") {
                put_error_response(&info_msg)
            } else if info_msg.contains("invalid") && info_msg.contains("command") {
                PutBluetoothResponses::BadRequest
            } else {
                value.into()
            }
        }
        Err(error) => {
            log::error!("put_bluetooth_device {} {} invalid request or response shape: {}", controller, device, error);
            PutBluetoothResponses::InternalError
        }
    }
}

/// `GET /api/v2/bluetooth/ws` — upgrade to the BLE notification websocket.
#[cfg(feature = "bluetooth-websocket")]
#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/bluetooth/ws",
    tag = "bluetooth",
    responses(crate::notifications::NotificationWebsocketResponse)
))]
pub async fn get_bluetooth_websocket(
    upgrade: Result<
        axum::extract::ws::WebSocketUpgrade,
        axum::extract::ws::rejection::WebSocketUpgradeRejection,
    >,
) -> crate::notifications::NotificationWebsocketResponse {
    super::shared::bluetooth_websocket_upgrade_response(upgrade)
        .await
        .expect("websocket upgrade should always return a response")
}

