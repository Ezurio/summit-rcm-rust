//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! VSP (Virtual Serial Port) device plugin — owns its typed request/response
//! schema and its `/api/v2/bluetooth/.../vsp` sub-resources. The shared
//! `/bluetooth` command endpoint forwards `gatt*` commands into the same
//! service pipeline for legacy compatibility.

use crate::routes::shared::{
    gatt_connections_response, BluetoothVspCommandRequest, BluetoothVspControlResponse,
};
use crate::service::list_vsp_connections;
use summit_rcm_bluetooth::service::{
    BluetoothCommandRouteError, BluetoothCommandRouteStatus, BluetoothService,
};
use summit_rcm_web::axum::{extract::Path, Json};
use summit_rcm_web::serde_json;

summit_rcm_web::define_json_response_family! {
    pub enum GetBluetoothVspResponses {
        Ok(BluetoothVspControlResponse) => 200;
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
    from BluetoothVspControlResponse => Ok;
}

summit_rcm_web::define_json_response_family! {
    pub enum PutBluetoothVspResponses {
        Ok(BluetoothVspControlResponse) => 200;
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
    from BluetoothVspControlResponse => Ok;
}

fn vsp_put_error_response(error: &BluetoothCommandRouteError) -> PutBluetoothVspResponses {
    match error.status() {
        BluetoothCommandRouteStatus::BadRequest => PutBluetoothVspResponses::BadRequest,
        BluetoothCommandRouteStatus::NotFound => PutBluetoothVspResponses::NotFound,
        BluetoothCommandRouteStatus::InternalError => PutBluetoothVspResponses::InternalError,
    }
}

async fn run_vsp_command(
    controller: &str,
    device: Option<&str>,
    mut body: serde_json::Map<String, serde_json::Value>,
    command: &str,
) -> PutBluetoothVspResponses {
    let _ = body.insert(
        "command".to_string(),
        serde_json::Value::String(command.to_string()),
    );
    match BluetoothService::run_command_value_v2(Some(controller), device, serde_json::Value::Object(body)).await {
        Ok(Ok((_response, _info))) => BluetoothVspControlResponse::default().into(),
        Ok(Err(error)) => vsp_put_error_response(&error),
        Err(error) => {
            log::error!("bluetooth vsp command {} failed: {}", command, error);
            PutBluetoothVspResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/bluetooth/{controller}/vsp",
    tag = "bluetooth",
    params(("controller" = String, Path, description = "Controller address")),
    responses(GetBluetoothVspResponses)
))]
pub async fn get_bluetooth_vsp(Path(_controller): Path<String>) -> GetBluetoothVspResponses {
    gatt_connections_response(list_vsp_connections().await).into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/bluetooth/{controller}/{device}/vsp",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("device" = String, Path, description = "Device address")
    ),
    request_body = BluetoothVspCommandRequest,
    responses(PutBluetoothVspResponses)
))]
pub async fn put_bluetooth_vsp(
    Path((controller, device)): Path<(String, String)>,
    Json(request): Json<BluetoothVspCommandRequest>,
) -> PutBluetoothVspResponses {
    let body = match serde_json::to_value(&request) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    run_vsp_command(&controller, Some(&device), body, "gattConnect").await
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/bluetooth/{controller}/{device}/vsp",
    tag = "bluetooth",
    params(
        ("controller" = String, Path, description = "Controller address"),
        ("device" = String, Path, description = "Device address")
    ),
    responses(PutBluetoothVspResponses)
))]
pub async fn delete_bluetooth_vsp(
    Path((controller, device)): Path<(String, String)>,
) -> PutBluetoothVspResponses {
    run_vsp_command(&controller, Some(&device), serde_json::Map::new(), "gattDisconnect").await
}

#[cfg(test)]
#[path = "../../tests/routes/v2/tests.rs"]
mod tests;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    get_bluetooth_vsp,
    put_bluetooth_vsp,
    delete_bluetooth_vsp,
))]
pub(crate) struct VspApiDoc;
