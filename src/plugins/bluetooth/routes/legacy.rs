//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::bluetooth::routes::shared::{
    include_filter, BluetoothCommandRequest, BluetoothControlResponse, BluetoothControllerState,
    BluetoothDeviceModel, BluetoothQuery,
};
use crate::plugins::bluetooth::service::{BluetoothDeviceStateError, BluetoothService};
use axum::{extract::{Path, Query}, Json};
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

impl BluetoothControllerState {
    pub fn into_legacy_controllers(self) -> BTreeMap<String, LegacyBluetoothControllerModel> {
        let filters = self.matched_filters.as_deref();
        let model = LegacyBluetoothControllerModel {
            bluetooth_devices: include_filter(filters, "bluetoothDevices").then_some(self.devices),
            discovering: include_filter(filters, "discovering").then_some(i32::from(self.discovering)),
            powered: include_filter(filters, "powered").then_some(i32::from(self.powered)),
            discoverable: include_filter(filters, "discoverable").then_some(i32::from(self.discoverable)),
            // Legacy exposes the cached `Transport` discovery filter under the
            // renamed `transportFilter` key.
            transport_filter: include_filter(filters, "transportFilter")
                .then_some(self.transport_filter)
                .flatten(),
        };
        BTreeMap::from([(self.controller_name, model)])
    }
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

fn legacy_bluetooth_state_response(
    operation: LegacyOperationResponse,
    controllers: BTreeMap<String, LegacyBluetoothControllerModel>,
) -> LegacyBluetoothStateResponse {
    LegacyBluetoothStateResponse {
        operation,
        controllers,
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
    BluetoothControlResponse::default()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/bluetooth",
    tag = "bluetooth",
    params(("filter" = Option<String>, Query, description = "Comma-separated response field filters")),
    responses(GetBluetoothLegacyResponses)
))]
pub async fn get_bluetooth_legacy(Query(query): Query<BluetoothQuery>) -> GetBluetoothLegacyResponses {
    match BluetoothService::get_controller_state(None, query.filters()).await {
        Ok(state) => legacy_bluetooth_state_response(ok_response(""), state.into_legacy_controllers()).into(),
        Err(error) => legacy_bluetooth_state_response(fail_response(error.to_string()), BTreeMap::new()).into(),
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
    match BluetoothService::handle_command_legacy(None, None, body).await {
        Ok((response, succeeded, info_msg)) => legacy_bluetooth_control_response(
            if succeeded { ok_response(info_msg) } else { fail_response(info_msg) },
            response,
        ).into(),
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
    match BluetoothService::get_controller_state(Some(&controller), query.filters()).await {
        Ok(state) => legacy_bluetooth_state_response(ok_response(""), state.into_legacy_controllers()).into(),
        Err(error) => legacy_bluetooth_state_response(fail_response(error.to_string()), BTreeMap::new()).into(),
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
    match BluetoothService::handle_command_legacy(Some(&controller), None, body).await {
        Ok((response, succeeded, info_msg)) => legacy_bluetooth_control_response(
            if succeeded { ok_response(info_msg) } else { fail_response(info_msg) },
            response,
        ).into(),
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

    match BluetoothService::handle_command_legacy(Some(&controller), Some(&device), body)
        .await
    {
        Ok((response, succeeded, info_msg)) => legacy_bluetooth_control_response(
            if succeeded { ok_response(info_msg) } else { fail_response(info_msg) },
            response,
        ).into(),
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

// Response for the legacy `/bluetoothWebsocket` index route: the standard
// legacy ack when notifications are enabled, otherwise 404.
#[cfg(feature = "bluetooth-websocket")]
crate::define_json_response_family! {
    pub enum BluetoothWebsocketIndexLegacyResponse {
        Ok(LegacyOperationResponse) => 200;
        NotFound => 404
    }
    from LegacyOperationResponse => Ok;
}

// Response for the legacy `/bluetoothWebsocket/ws` route. The `200` arm is a
// CherryPy parity quirk: a non-upgrade GET to the websocket URL is answered
// with the standard legacy ack instead of switching protocols.
#[cfg(feature = "bluetooth-websocket")]
crate::define_websocket_response_family! {
    pub enum BluetoothWebsocketLegacyResponse {
        Upgrade => 101, "Switching protocols";
        Ok(LegacyOperationResponse) => 200;
        NotFound => 404
    }
    from LegacyOperationResponse => Ok;
}

/// `GET /bluetoothWebsocket` (and trailing-slash form) — non-upgrade index that
/// acknowledges with the standard legacy body when notifications are enabled.
#[cfg(feature = "bluetooth-websocket")]
#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/bluetoothWebsocket",
    tag = "bluetooth",
    responses(BluetoothWebsocketIndexLegacyResponse)
))]
pub async fn get_bluetooth_websocket_index_legacy() -> BluetoothWebsocketIndexLegacyResponse {
    if !BluetoothService::websocket_notifications_enabled() {
        return BluetoothWebsocketIndexLegacyResponse::NotFound;
    }

    BluetoothWebsocketIndexLegacyResponse::from(ok_response(""))
}

/// `GET /bluetoothWebsocket/ws` — upgrade to the BLE notification websocket, or
/// answer a non-upgrade GET with the standard legacy ack.
#[cfg(feature = "bluetooth-websocket")]
#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/bluetoothWebsocket/ws",
    tag = "bluetooth",
    responses(BluetoothWebsocketLegacyResponse)
))]
pub async fn get_bluetooth_websocket_legacy(
    upgrade: Result<
        axum::extract::ws::WebSocketUpgrade,
        axum::extract::ws::rejection::WebSocketUpgradeRejection,
    >,
) -> BluetoothWebsocketLegacyResponse {
    use crate::notifications::NotificationWebsocketResponse;

    match super::shared::bluetooth_websocket_upgrade_response(upgrade).await {
        Some(NotificationWebsocketResponse::Upgrade(response)) => {
            BluetoothWebsocketLegacyResponse::Upgrade(response)
        }
        Some(NotificationWebsocketResponse::NotFound) => BluetoothWebsocketLegacyResponse::NotFound,
        None => BluetoothWebsocketLegacyResponse::from(ok_response("")),
    }
}

