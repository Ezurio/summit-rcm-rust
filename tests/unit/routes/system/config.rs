use super::export_config;
use axum::response::IntoResponse;

#[tokio::test]
async fn export_config_requires_password() {
    let response = export_config(None).await.into_response();

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}