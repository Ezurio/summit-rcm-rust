use super::delete_bluetooth_hid;
use super::put_bluetooth_hid;
use crate::routes::shared::BluetoothHidCommandRequest;
use summit_rcm_web::axum::{extract::Path, response::IntoResponse, Json};

#[tokio::test]
async fn put_missing_tcp_port_returns_bad_request() {
    let response = put_bluetooth_hid(
        Path(("controller0".to_string(), "AA:BB:CC:DD:EE:FF".to_string())),
        Json(BluetoothHidCommandRequest::default()),
    )
    .await
    .into_response();

    assert_eq!(response.status(), summit_rcm_web::axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_without_connection_returns_internal_error() {
    let response = delete_bluetooth_hid(Path((
        "controller0".to_string(),
        "AA:BB:CC:DD:EE:FF".to_string(),
    )))
    .await
    .into_response();

    assert_eq!(response.status(), summit_rcm_web::axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}