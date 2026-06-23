use super::export_config;
use summit_rcm_web::axum::response::IntoResponse;

#[tokio::test]
async fn export_config_requires_password() {
    let response = export_config(None).await.into_response();

    assert_eq!(response.status(), summit_rcm_web::axum::http::StatusCode::BAD_REQUEST);
}
