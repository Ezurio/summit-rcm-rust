//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::put_cww_legacy;
use summit_rcm_web::axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::put,
    Router,
};
use tower::ServiceExt;
use summit_rcm_web::serde_json;

#[tokio::test]
async fn legacy_cww_missing_name_returns_python_error_body() {
    let app = Router::new().route("/cww", put(put_cww_legacy));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/cww")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["SDCERR"], 1);
    assert_eq!(value["InfoMsg"], "Invalid interface name");
}
