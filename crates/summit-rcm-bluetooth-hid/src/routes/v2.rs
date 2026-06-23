//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! HID device plugin — owns its typed request/response schema and its
//! `/api/v2/bluetooth/.../hid` sub-resources. The shared `/bluetooth` command
//! endpoint forwards `hid*` commands into the same service pipeline for legacy
//! compatibility.

use crate::routes::shared::{
    hid_connections_response, BluetoothHidCommandRequest, BluetoothHidControlResponse,
};
use crate::service::list_hid_connections;
use summit_rcm_bluetooth::service::{
    BluetoothCommandRouteError, BluetoothCommandRouteStatus, BluetoothService,
};
use summit_rcm_web::axum::{extract::Path, Json};
use summit_rcm_web::serde_json;

summit_rcm_web::define_json_response_family! {
    pub enum GetBluetoothHidResponses {
        Ok(BluetoothHidControlResponse) => 200;
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
    from BluetoothHidControlResponse => Ok;
}

summit_rcm_web::define_json_response_family! {
    pub enum PutBluetoothHidResponses {
        Ok(BluetoothHidControlResponse) => 200;
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
    from BluetoothHidControlResponse => Ok;
}

fn hid_put_error_response(error: &BluetoothCommandRouteError) -> PutBluetoothHidResponses {
    match error.status() {
        BluetoothCommandRouteStatus::BadRequest => PutBluetoothHidResponses::BadRequest,
        BluetoothCommandRouteStatus::NotFound => PutBluetoothHidResponses::NotFound,
        BluetoothCommandRouteStatus::InternalError => PutBluetoothHidResponses::InternalError,
    }
}

async fn run_hid_command(
    controller: &str,
    device: Option<&str>,
    mut body: serde_json::Map<String, serde_json::Value>,
    command: &str,
) -> PutBluetoothHidResponses {
    let _ = body.insert(
        "command".to_string(),
        serde_json::Value::String(command.to_string()),
    );
    match BluetoothService::run_command_value_v2(Some(controller), device, serde_json::Value::Object(body)).await {
        Ok(Ok((_response, _info))) => BluetoothHidControlResponse::default().into(),
        Ok(Err(error)) => hid_put_error_response(&error),
        Err(error) => {
            log::error!("bluetooth hid command {} failed: {}", command, error);
            PutBluetoothHidResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/bluetooth/{controller}/hid",
    tag = "bluetooth",
    params(("controller" = String, Path, description = "Controller address")),
    responses(GetBluetoothHidResponses)
))]
pub async fn get_bluetooth_hid(Path(_controller): Path<String>) -> GetBluetoothHidResponses {
    hid_connections_response(list_hid_connections().await).into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/bluetooth/{controller}/{device}/hid",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("device" = String, Path, description = "Device address")
    ),
    request_body = BluetoothHidCommandRequest,
    responses(PutBluetoothHidResponses)
))]
pub async fn put_bluetooth_hid(
    Path((controller, device)): Path<(String, String)>,
    Json(request): Json<BluetoothHidCommandRequest>,
) -> PutBluetoothHidResponses {
    let body = match serde_json::to_value(&request) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    run_hid_command(&controller, Some(&device), body, "hidConnect").await
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/bluetooth/{controller}/{device}/hid",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("device" = String, Path, description = "Device address")
    ),
    responses(PutBluetoothHidResponses)
))]
pub async fn delete_bluetooth_hid(
    Path((controller, device)): Path<(String, String)>,
) -> PutBluetoothHidResponses {
    run_hid_command(&controller, Some(&device), serde_json::Map::new(), "hidDisconnect").await
}

#[cfg(test)]
#[path = "../../tests/routes/v2/tests.rs"]
mod tests;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    get_bluetooth_hid,
    put_bluetooth_hid,
    delete_bluetooth_hid,
))]
pub(crate) struct HidApiDoc;
