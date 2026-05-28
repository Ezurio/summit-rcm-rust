//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::bluetooth::routes::common::{
    BluetoothCommandRequest, BluetoothControlResponse,
    BluetoothDeviceModel, BluetoothQuery,
};
#[cfg(feature = "bluetooth-websocket")]
use crate::plugins::bluetooth::routes::websocket::bluetooth_websocket_upgrade_response;
use crate::plugins::bluetooth::service::{BluetoothDeviceStateError, BluetoothService};
use crate::utils::parse_model;
use axum::{extract::{Path, Query}, Json};
#[cfg(feature = "bluetooth-websocket")]
use axum::extract::ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade};
#[cfg(feature = "bluetooth-websocket")]
use axum::http::StatusCode;
#[cfg(feature = "bluetooth-websocket")]
use axum::response::{IntoResponse, Response};
#[cfg(feature = "bluetooth-websocket")]
use axum::Json as AxumJson;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyBluetoothControllerModel {
    #[serde(rename = "bluetoothDevices")]
    pub bluetooth_devices: Option<Vec<BluetoothDeviceModel>>,
    pub discovering: Option<i32>,
    pub powered: Option<i32>,
    pub discoverable: Option<i32>,
    #[serde(rename = "transportFilter")]
    pub transport_filter: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyBluetoothStateResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(flatten)]
    pub controllers: BTreeMap<String, LegacyBluetoothControllerModel>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyBluetoothControlResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(flatten)]
    pub response: BluetoothControlResponse,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyBluetoothDeviceResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(flatten)]
    pub device: BluetoothDeviceModel,
}

define_bluetooth_legacy_response_family! {
    pub enum GetBluetoothLegacyResponses(LegacyBluetoothStateResponse);
}
define_bluetooth_legacy_response_family! {
    pub enum PutBluetoothLegacyResponses(LegacyBluetoothControlResponse);
}
define_bluetooth_legacy_response_family! {
    pub enum GetBluetoothDeviceLegacyResponses(LegacyBluetoothDeviceResponse);
}

pub type PutBluetoothDeviceLegacyResponses = PutBluetoothLegacyResponses;

fn legacy_bluetooth_control_response(
    operation: LegacyOperationResponse,
    response: BluetoothControlResponse,
) -> LegacyBluetoothControlResponse {
    LegacyBluetoothControlResponse {
        operation,
        response,
    }
}

fn legacy_bluetooth_device_response(
    operation: LegacyOperationResponse,
    device: BluetoothDeviceModel,
) -> LegacyBluetoothDeviceResponse {
    LegacyBluetoothDeviceResponse {
        operation,
        device,
    }
}

fn parse_legacy_response<T, U>(value: U) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
    U: Serialize,
{
    parse_model(value)
}

fn parse_legacy_route_response<T, U, R, F>(
    value: U,
    log_context: impl AsRef<str>,
    bad_request: F,
) -> R
where
    T: DeserializeOwned,
    U: Serialize,
    R: From<T>,
    F: FnOnce() -> R,
{
    match parse_legacy_response::<T, U>(value) {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("{}: {}", log_context.as_ref(), error);
            bad_request()
        }
    }
}

fn empty_bluetooth_device() -> BluetoothDeviceModel {
    BluetoothDeviceModel {
        auto_connect: None,
        auto_connect_auto_disable: None,
        address: None,
        address_type: None,
        name: None,
        alias: None,
        paired: None,
        bonded: None,
        trusted: None,
        blocked: None,
        legacy_pairing: None,
        rssi: None,
        connected: None,
        uuids: None,
        adapter: None,
        manufacturer_data: None,
        service_data: None,
        services_resolved: None,
    }
}

fn empty_bluetooth_control() -> BluetoothControlResponse {
    BluetoothControlResponse {
        rssi: None,
        tx_power: None,
        max_tx_power: None,
        #[cfg(feature = "bluetooth-hid")]
        hid: crate::plugins::bluetooth::routes::hid::BluetoothHidControlResponse {
            hid_connections: None,
        },
        started: None,
        port: None,
        #[cfg(feature = "bluetooth-vsp")]
        vsp: crate::plugins::bluetooth::routes::vsp::BluetoothVspControlResponse {
            gatt_connections: None,
        },
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/bluetooth",
    tag = "bluetooth",
    params(("filter" = Option<String>, Query, description = "Comma-separated response field filters")),
    responses(GetBluetoothLegacyResponses)
))]
pub async fn get_bluetooth_legacy(Query(query): Query<BluetoothQuery>) -> GetBluetoothLegacyResponses {
    parse_legacy_route_response::<LegacyBluetoothStateResponse, _, _, _>(
        BluetoothService::get_state_legacy(None, None, query.filters()).await,
        "get_bluetooth_legacy invalid response shape",
        || GetBluetoothLegacyResponses::BadRequest,
    )
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/bluetooth",
    tag = "bluetooth",
    request_body = BluetoothCommandRequest,
    responses(PutBluetoothLegacyResponses)
))]
pub async fn put_bluetooth_legacy(Json(body): Json<BluetoothCommandRequest>) -> PutBluetoothLegacyResponses {
    match BluetoothService::handle_command_json(None, None, body).await {
        Ok(value) => parse_legacy_route_response::<LegacyBluetoothControlResponse, _, _, _>(
            value,
            "put_bluetooth_legacy invalid request or response shape",
            || PutBluetoothLegacyResponses::BadRequest,
        ),
        Err(error) => {
            log::error!("put_bluetooth_legacy invalid request or response shape: {}", error);
            PutBluetoothLegacyResponses::BadRequest
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/bluetooth/{controller}",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("filter" = Option<String>, Query, description = "Comma-separated response field filters")
    ),
    responses(GetBluetoothLegacyResponses)
))]
pub async fn get_bluetooth_controller_legacy(
    Path(controller): Path<String>,
    Query(query): Query<BluetoothQuery>,
) -> GetBluetoothLegacyResponses {
    parse_legacy_route_response::<LegacyBluetoothStateResponse, _, _, _>(
        BluetoothService::get_state_legacy(Some(&controller), None, query.filters()).await,
        format!("get_bluetooth_controller_legacy {} invalid response shape", controller),
        || GetBluetoothLegacyResponses::BadRequest,
    )
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/bluetooth/{controller}",
    tag = "bluetooth",
    params(("controller" = String, Path, description = "Controller address")),
    request_body = BluetoothCommandRequest,
    responses(PutBluetoothLegacyResponses)
))]
pub async fn put_bluetooth_controller_legacy(
    Path(controller): Path<String>,
    Json(body): Json<BluetoothCommandRequest>,
) -> PutBluetoothLegacyResponses {
    match BluetoothService::handle_command_json(Some(&controller), None, body).await {
        Ok(value) => parse_legacy_route_response::<LegacyBluetoothControlResponse, _, _, _>(
            value,
            format!(
                "put_bluetooth_controller_legacy {} invalid request or response shape",
                controller
            ),
            || PutBluetoothLegacyResponses::BadRequest,
        ),
        Err(error) => {
            log::error!(
                "put_bluetooth_controller_legacy {} invalid request or response shape: {}",
                controller,
                error
            );
            PutBluetoothLegacyResponses::BadRequest
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/bluetooth/{controller}/{device}",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("device" = String, Path, description = "Device address"),
        ("filter" = Option<String>, Query, description = "Comma-separated response field filters")
    ),
    responses(GetBluetoothLegacyResponses)
))]
pub async fn get_bluetooth_device_legacy(
    Path((controller, device)): Path<(String, String)>,
) -> GetBluetoothDeviceLegacyResponses {
    match BluetoothService::get_device_state_typed(&controller, &device).await {
        Ok(device) => legacy_bluetooth_device_response(ok_response(""), device).into(),
        Err(BluetoothDeviceStateError::ControllerNotFound | BluetoothDeviceStateError::DeviceNotFound) => {
            legacy_bluetooth_device_response(fail_response("Device not found"), empty_bluetooth_device()).into()
        }
        Err(error) => {
            log::error!("get_bluetooth_device_legacy {} {}: {:?}", controller, device, error);
            GetBluetoothDeviceLegacyResponses::BadRequest
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/bluetooth/{controller}/{device}",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("device" = String, Path, description = "Device address")
    ),
    request_body = BluetoothCommandRequest,
    responses(PutBluetoothDeviceLegacyResponses)
))]
pub async fn put_bluetooth_device_legacy(
    Path((controller, device)): Path<(String, String)>,
    Json(body): Json<BluetoothCommandRequest>,
) -> PutBluetoothDeviceLegacyResponses {
    if BluetoothService::get_device_state_typed(&controller, &device).await.is_err() {
        return legacy_bluetooth_control_response(
            fail_response("Device not found"),
            empty_bluetooth_control(),
        )
        .into();
    }

    match BluetoothService::handle_command_json(Some(&controller), Some(&device), body)
        .await
    {
        Ok(value) => parse_legacy_route_response::<LegacyBluetoothControlResponse, _, _, _>(
            value,
            format!(
                "put_bluetooth_device_legacy {} {} invalid request or response shape",
                controller, device
            ),
            || PutBluetoothDeviceLegacyResponses::BadRequest,
        ),
        Err(error) => {
            log::error!(
                "put_bluetooth_device_legacy {} {} invalid request or response shape: {}",
                controller,
                device,
                error
            );
            PutBluetoothDeviceLegacyResponses::BadRequest
        }
    }
}

#[cfg(feature = "bluetooth-websocket")]
#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/bluetoothWebsocket",
    tag = "bluetooth",
    responses(
        (status = 200, body = LegacyOperationResponse),
        (status = 404, description = "Bluetooth websocket notifications not enabled")
    )
))]
pub async fn get_bluetooth_websocket_index_legacy() -> Response {
    if !BluetoothService::websocket_notifications_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }

    AxumJson(ok_response("")).into_response()
}

#[cfg(feature = "bluetooth-websocket")]
pub async fn get_bluetooth_websocket_legacy(
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    if let Some(response) = bluetooth_websocket_upgrade_response(upgrade).await {
        response
    } else {
        AxumJson(ok_response("")).into_response()
    }
}
