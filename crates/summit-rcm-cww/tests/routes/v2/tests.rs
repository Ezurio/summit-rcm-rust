//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::put_interface_cww;
use summit_rcm_web::axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::put,
};
use summit_rcm_web::serde_json;
use tower::ServiceExt;

#[tokio::test]
async fn v2_cww_false_request_returns_noop_response() {
    let app = Router::new().route(
        "/api/v2/network/interfaces/{name}/cww",
        put(put_interface_cww),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v2/network/interfaces/wlan0/cww")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"changeToWorldWide":false,"autoReboot":true}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["changeToWorldWide"], false);
    assert_eq!(value["autoReboot"], false);
}
