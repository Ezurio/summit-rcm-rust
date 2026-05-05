//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::bluetooth::routes::common::{
    parse_bluetooth_control_response, parse_bluetooth_device_response,
    parse_bluetooth_state_response, BluetoothCommandRequest, BluetoothControlResponse,
    BluetoothDeviceModel, BluetoothQuery, BluetoothStateResponse,
};
#[cfg(feature = "bluetooth-websocket")]
use crate::plugins::bluetooth::routes::common::bluetooth_websocket_upgrade_response;
use crate::plugins::bluetooth::service::BluetoothService;
use axum::{extract::{Path, Query}, Json};
#[cfg(feature = "bluetooth-websocket")]
use axum::extract::ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade};
#[cfg(feature = "bluetooth-websocket")]
use axum::response::Response;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

crate::define_json_response_family! {
    pub enum GetBluetoothResponses {
        Ok(BluetoothStateResponse) => 200;
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
    from BluetoothStateResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutBluetoothResponses {
        Ok(BluetoothControlResponse) => 200;
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
    from BluetoothControlResponse => Ok;
}

crate::define_json_response_family! {
    pub enum GetBluetoothDeviceResponses {
        Ok(BluetoothDeviceModel) => 200;
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
    from BluetoothDeviceModel => Ok;
}

fn get_error_response(error: &anyhow::Error) -> GetBluetoothResponses {
    let message = error.to_string();
    if message.contains("controller not found") {
        GetBluetoothResponses::NotFound
    } else if message.contains("device not found") || message.contains("filters") {
        GetBluetoothResponses::BadRequest
    } else {
        GetBluetoothResponses::InternalError
    }
}

fn get_device_error_response(error: &anyhow::Error) -> GetBluetoothDeviceResponses {
    let message = error.to_string();
    if message.contains("controller not found") {
        GetBluetoothDeviceResponses::NotFound
    } else if message.contains("device not found") || message.contains("filters") {
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
    match BluetoothService::get_state_v2_result(None, None, query.filters()).await {
        Ok(value) => match parse_bluetooth_state_response(value) {
            Ok(value) => value.into(),
            Err(error) => {
                tracing::error!("get_bluetooth invalid response shape: {}", error);
                GetBluetoothResponses::InternalError
            }
        },
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
    let body = match serde_json::to_value(body) {
        Ok(body) => body,
        Err(error) => {
            tracing::error!("put_bluetooth invalid request shape: {}", error);
            return PutBluetoothResponses::BadRequest;
        }
    };

    let value = BluetoothService::handle_command(None, None, &body).await;
    let info_msg = value
        .get("InfoMsg")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match parse_bluetooth_control_response(value) {
        Ok(value) => {
            if info_msg.contains("No Bluetooth adapter found") || info_msg.contains("controller not found") {
                PutBluetoothResponses::NotFound
            } else if info_msg.contains("Unknown command:") {
                PutBluetoothResponses::BadRequest
            } else {
                value.into()
            }
        }
        Err(error) => {
            tracing::error!("put_bluetooth invalid response shape: {}", error);
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
    match BluetoothService::get_state_v2_result(Some(&controller), None, query.filters()).await {
        Ok(value) => match parse_bluetooth_state_response(value) {
            Ok(value) => value.into(),
            Err(error) => {
                tracing::error!("get_bluetooth_controller {} invalid response shape: {}", controller, error);
                GetBluetoothResponses::InternalError
            }
        },
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
    let body = match serde_json::to_value(body) {
        Ok(body) => body,
        Err(error) => {
            tracing::error!("put_bluetooth_controller {} invalid request shape: {}", controller, error);
            return PutBluetoothResponses::BadRequest;
        }
    };

    let value = BluetoothService::handle_command(Some(&controller), None, &body).await;
    let info_msg = value
        .get("InfoMsg")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match parse_bluetooth_control_response(value) {
        Ok(value) => {
            if info_msg.contains("No Bluetooth adapter found") || info_msg.contains("controller not found") {
                put_error_response(&info_msg)
            } else {
                value.into()
            }
        }
        Err(error) => {
            tracing::error!("put_bluetooth_controller {} invalid response shape: {}", controller, error);
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
    match BluetoothService::get_device_state_v2(&controller, &device).await {
        Ok(value) => match parse_bluetooth_device_response(value) {
            Ok(value) => value.into(),
            Err(error) => {
                tracing::error!("get_bluetooth_device {} {} invalid response shape: {}", controller, device, error);
                GetBluetoothDeviceResponses::InternalError
            }
        },
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
    let body = match serde_json::to_value(body) {
        Ok(body) => body,
        Err(error) => {
            tracing::error!("put_bluetooth_device {} {} invalid request shape: {}", controller, device, error);
            return PutBluetoothResponses::BadRequest;
        }
    };

    let value = BluetoothService::handle_command(Some(&controller), Some(&device), &body).await;
    let info_msg = value
        .get("InfoMsg")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match parse_bluetooth_control_response(value) {
        Ok(value) => {
            if info_msg.contains("No Bluetooth adapter found") || info_msg.contains("controller not found") {
                put_error_response(&info_msg)
            } else if info_msg.contains("invalid") && info_msg.contains("command") {
                PutBluetoothResponses::BadRequest
            } else {
                value.into()
            }
        }
        Err(error) => {
            tracing::error!("put_bluetooth_device {} {} invalid response shape: {}", controller, device, error);
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