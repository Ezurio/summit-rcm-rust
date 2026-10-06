use super::delete_bluetooth_vsp;
use super::put_bluetooth_vsp;
use crate::routes::shared::BluetoothVspCommandRequest;
use summit_rcm_web::axum::{Json, extract::Path, response::IntoResponse};

#[tokio::test]
async fn put_missing_service_uuid_returns_bad_request() {
    let response = put_bluetooth_vsp(
        Path(("controller0".to_string(), "AA:BB:CC:DD:EE:FF".to_string())),
        Json(BluetoothVspCommandRequest::default()),
    )
    .await
    .into_response();

    assert_eq!(
        response.status(),
        summit_rcm_web::axum::http::StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn delete_without_connection_returns_internal_error() {
    let response = delete_bluetooth_vsp(Path((
        "controller0".to_string(),
        "AA:BB:CC:DD:EE:FF".to_string(),
    )))
    .await
    .into_response();

    assert_eq!(
        response.status(),
        summit_rcm_web::axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
}
