use super::export_connections_route;
use summit_rcm_web::axum::{extract::Query, response::IntoResponse};
use std::collections::HashMap;

#[tokio::test]
async fn export_connections_requires_password() {
    let response = export_connections_route(Query(HashMap::<String, String>::new()), None)
        .await
        .into_response();

    assert_eq!(response.status(), summit_rcm_web::axum::http::StatusCode::BAD_REQUEST);
}
