//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::routes::shared::{
    BluetoothCommandRequest, BluetoothControlResponse, BluetoothDeviceModel, BluetoothQuery,
};
use crate::service::{
    BluetoothCommandRouteError, BluetoothCommandRouteStatus, BluetoothDeviceStateError,
    BluetoothService,
};
use summit_rcm_web::axum::{extract::{Path, Query}, Json};
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

define_bluetooth_v2_response_family! {
    pub enum GetBluetoothResponses(BluetoothStateResponse);
}

define_bluetooth_v2_response_family! {
    pub enum GetBluetoothDeviceResponses(BluetoothDeviceModel);
}

define_bluetooth_v2_response_family! {
    pub enum PutBluetoothResponses(BluetoothControlResponse);
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

fn put_error_response(error: &BluetoothCommandRouteError) -> PutBluetoothResponses {
    match error.status() {
        BluetoothCommandRouteStatus::BadRequest => PutBluetoothResponses::BadRequest,
        BluetoothCommandRouteStatus::NotFound => PutBluetoothResponses::NotFound,
        BluetoothCommandRouteStatus::InternalError => PutBluetoothResponses::InternalError,
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
    match BluetoothService::get_controller_state_v2_response(None, query.filters()).await {
        Ok(state) => state.into(),
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
        Ok(Ok((value, _info_msg))) => value.into(),
        Ok(Err(error)) => put_error_response(&error),
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
    match BluetoothService::get_controller_state_v2_response(Some(&controller), query.filters()).await {
        Ok(state) => state.into(),
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
        Ok(Ok((value, _info_msg))) => value.into(),
        Ok(Err(error)) => put_error_response(&error),
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
        ("device" = String, Path, description = "Device address"),
        ("filter" = Option<String>, Query, description = "Comma-separated response field filters")
    ),
    responses(GetBluetoothDeviceResponses)
))]
pub async fn get_bluetooth_device(
    Path((controller, device)): Path<(String, String)>,
    Query(_query): Query<BluetoothQuery>,
) -> GetBluetoothDeviceResponses {
    match BluetoothService::get_device_state_typed(&controller, &device).await {
        Ok(device) => device.into(),
        Err(BluetoothDeviceStateError::ControllerNotFound) => GetBluetoothDeviceResponses::NotFound,
        Err(BluetoothDeviceStateError::DeviceNotFound) => GetBluetoothDeviceResponses::BadRequest,
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
        Ok(Ok((value, _info_msg))) => value.into(),
        Ok(Err(error)) => put_error_response(&error),
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
    responses(summit_rcm_web::notifications::NotificationWebsocketResponse)
))]
pub async fn get_bluetooth_websocket(
    upgrade: Result<
        summit_rcm_web::axum::extract::ws::WebSocketUpgrade,
        summit_rcm_web::axum::extract::ws::rejection::WebSocketUpgradeRejection,
    >,
) -> summit_rcm_web::notifications::NotificationWebsocketResponse {
    super::shared::bluetooth_websocket_upgrade_response(upgrade)
        .await
        .expect("websocket upgrade should always return a response")
}
