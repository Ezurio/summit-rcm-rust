#![cfg(any(feature = "api-v2", feature = "api-legacy"))]

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::put,
};
use tower::ServiceExt;

#[cfg(feature = "api-v2")]
#[tokio::test]
async fn v2_cww_false_request_returns_noop_response() {
    let app = Router::new().route(
        "/api/v2/network/interfaces/{name}/cww",
        put(summit_rcm::plugins::cww::routes::v2::put_interface_cww),
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

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_cww_missing_name_returns_python_error_body() {
    let app = Router::new().route(
        "/cww",
        put(summit_rcm::plugins::cww::routes::legacy::put_cww_legacy),
    );

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