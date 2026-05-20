//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::bluetooth::routes::common::{
    parse_bluetooth_device_response, BluetoothCommandRequest, BluetoothControlResponse,
    BluetoothDeviceModel, BluetoothQuery,
};
#[cfg(feature = "bluetooth-websocket")]
use crate::plugins::bluetooth::routes::websocket::bluetooth_websocket_upgrade_response;
use crate::plugins::bluetooth::service::BluetoothService;
use axum::{extract::{Path, Query}, Json};
#[cfg(feature = "bluetooth-websocket")]
use axum::extract::ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade};
#[cfg(feature = "bluetooth-websocket")]
use axum::response::{IntoResponse, Response};
#[cfg(feature = "bluetooth-websocket")]
use axum::Json as AxumJson;
use serde::{Deserialize, Serialize};
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
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(flatten)]
    pub controllers: BTreeMap<String, LegacyBluetoothControllerModel>,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyBluetoothControlResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(flatten)]
    pub response: BluetoothControlResponse,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyBluetoothDeviceResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(flatten)]
    pub device: BluetoothDeviceModel,
}

crate::define_json_response_family! {
    pub enum GetBluetoothLegacyResponses {
        Ok(LegacyBluetoothStateResponse) => 200;
        BadRequest => 400
    }
    from LegacyBluetoothStateResponse => Ok;
}
crate::define_json_response_family! {
    pub enum PutBluetoothLegacyResponses {
        Ok(LegacyBluetoothControlResponse) => 200;
        BadRequest => 400
    }
    from LegacyBluetoothControlResponse => Ok;
}
crate::define_json_response_family! {
    pub enum GetBluetoothDeviceLegacyResponses {
        Ok(LegacyBluetoothDeviceResponse) => 200;
        BadRequest => 400
    }
    from LegacyBluetoothDeviceResponse => Ok;
}
crate::define_json_response_family! {
    pub enum PutBluetoothDeviceLegacyResponses {
        Ok(LegacyBluetoothControlResponse) => 200;
        BadRequest => 400
    }
    from LegacyBluetoothControlResponse => Ok;
}

fn legacy_bluetooth_control_response(
    operation: LegacyOperationResponse,
    response: BluetoothControlResponse,
) -> LegacyBluetoothControlResponse {
    LegacyBluetoothControlResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        response,
    }
}

fn legacy_bluetooth_device_response(
    operation: LegacyOperationResponse,
    device: BluetoothDeviceModel,
) -> LegacyBluetoothDeviceResponse {
    LegacyBluetoothDeviceResponse {
        sdcerr: operation.sdcerr,
        info_msg: operation.info_msg,
        device,
    }
}

fn parse_legacy_bluetooth_state_response<T: Serialize>(value: T) -> Result<LegacyBluetoothStateResponse, serde_json::Error> {
    serde_json::from_str(&serde_json::to_string(&value)?)
}

fn parse_legacy_bluetooth_control_response<T: Serialize>(value: T) -> Result<LegacyBluetoothControlResponse, serde_json::Error> {
    serde_json::from_str(&serde_json::to_string(&value)?)
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/bluetooth",
    tag = "bluetooth",
    params(("filter" = Option<String>, Query, description = "Comma-separated response field filters")),
    responses(GetBluetoothLegacyResponses)
))]
pub async fn get_bluetooth_legacy(Query(query): Query<BluetoothQuery>) -> GetBluetoothLegacyResponses {
    match parse_legacy_bluetooth_state_response(BluetoothService::get_state_legacy(None, None, query.filters()).await) {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("get_bluetooth_legacy invalid response shape: {}", error);
            GetBluetoothLegacyResponses::BadRequest
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/bluetooth",
    tag = "bluetooth",
    request_body = BluetoothCommandRequest,
    responses(PutBluetoothLegacyResponses)
))]
pub async fn put_bluetooth_legacy(Json(body): Json<BluetoothCommandRequest>) -> PutBluetoothLegacyResponses {
    let body = match serde_json::to_value(body) {
        Ok(body) => body,
        Err(error) => {
            log::error!("put_bluetooth_legacy invalid request shape: {}", error);
            return PutBluetoothLegacyResponses::BadRequest;
        }
    };
    match parse_legacy_bluetooth_control_response(BluetoothService::handle_command(None, None, &body).await) {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("put_bluetooth_legacy invalid response shape: {}", error);
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
    match parse_legacy_bluetooth_state_response(BluetoothService::get_state_legacy(Some(&controller), None, query.filters()).await) {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("get_bluetooth_controller_legacy {} invalid response shape: {}", controller, error);
            GetBluetoothLegacyResponses::BadRequest
        }
    }
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
    let body = match serde_json::to_value(body) {
        Ok(body) => body,
        Err(error) => {
            log::error!("put_bluetooth_controller_legacy {} invalid request shape: {}", controller, error);
            return PutBluetoothLegacyResponses::BadRequest;
        }
    };
    match parse_legacy_bluetooth_control_response(BluetoothService::handle_command(Some(&controller), None, &body).await) {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("put_bluetooth_controller_legacy {} invalid response shape: {}", controller, error);
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
    if BluetoothService::get_device_state_v2(&controller, &device).await.is_err() {
        return legacy_bluetooth_device_response(
            fail_response("Device not found"),
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
            },
        )
        .into();
    }

    match BluetoothService::get_device_state_v2(&controller, &device).await.and_then(|value| parse_bluetooth_device_response(value).map_err(Into::into)) {
        Ok(device) => legacy_bluetooth_device_response(ok_response(""), device).into(),
        Err(error) => {
            log::error!("get_bluetooth_device_legacy {} {}: {}", controller, device, error);
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
    if BluetoothService::get_device_state_v2(&controller, &device).await.is_err() {
        return legacy_bluetooth_control_response(
            fail_response("Device not found"),
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
            },
        )
        .into();
    }

    let body = match serde_json::to_value(body) {
        Ok(body) => body,
        Err(error) => {
            log::error!("put_bluetooth_device_legacy {} {} invalid request shape: {}", controller, device, error);
            return PutBluetoothDeviceLegacyResponses::BadRequest;
        }
    };
    match parse_legacy_bluetooth_control_response(BluetoothService::handle_command(Some(&controller), Some(&device), &body).await) {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("put_bluetooth_device_legacy {} {} invalid response shape: {}", controller, device, error);
            PutBluetoothDeviceLegacyResponses::BadRequest
        }
    }
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
