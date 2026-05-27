//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::bluetooth::routes::common::{
    BluetoothCommandRequest, BluetoothControlResponse, BluetoothDeviceModel,
    BluetoothQuery, BluetoothStateResponse,
};
#[cfg(feature = "bluetooth-websocket")]
use crate::plugins::bluetooth::routes::websocket::bluetooth_websocket_upgrade_response;
use crate::plugins::bluetooth::service::BluetoothService;
use axum::{extract::{Path, Query}, Json};
#[cfg(feature = "bluetooth-websocket")]
use axum::extract::ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade};
#[cfg(feature = "bluetooth-websocket")]
use axum::response::Response;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

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

fn get_device_error_response(error: &anyhow::Error) -> GetBluetoothDeviceResponses {
    let message = error.to_string();
    if message.contains("controller not found") || message.contains("device not found") {
        GetBluetoothDeviceResponses::NotFound
    } else if message.contains("filters") {
        GetBluetoothDeviceResponses::BadRequest
    } else {
        GetBluetoothDeviceResponses::InternalError
    }
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
    match BluetoothService::get_state_v2(None, None, query.filters()).await {
        Ok(value) => value.into(),
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
    match BluetoothService::get_state_v2(Some(&controller), None, query.filters()).await {
        Ok(value) => value.into(),
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
    match BluetoothService::get_device_state(&controller, &device).await {
        Ok(value) => value.into(),
        Err(error) => get_device_error_response(&error),
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

#[cfg(feature = "bluetooth-websocket")]
pub async fn get_bluetooth_websocket(
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    bluetooth_websocket_upgrade_response(upgrade)
        .await
        .expect("websocket upgrade should always return a response")
}