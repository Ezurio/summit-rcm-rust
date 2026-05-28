#[cfg(all(feature = "unauthenticated", feature = "api-v2"))]
#[tokio::test]
async fn v2_unauthenticated_get_does_not_return_sdcerr() {
    use axum::{body::to_bytes, response::IntoResponse};
    use serde_json::Value;

    let response = summit_rcm::plugins::unauthenticated::routes::v2::get_unauthenticated()
        .await
        .into_response();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: Value = serde_json::from_slice(&body).unwrap();

    assert!(payload.get("SDCERR").is_none());
    assert!(payload.get("InfoMsg").is_none());
    assert!(payload.get("allowUnauthenticatedRebootReset").is_some());
}

#[cfg(all(feature = "unauthenticated", feature = "api-v2"))]
#[tokio::test]
async fn v2_unauthenticated_put_delete_do_not_return_sdcerr() {
    use axum::{body::to_bytes, response::IntoResponse};

    let put_response = summit_rcm::plugins::unauthenticated::routes::v2::put_unauthenticated()
        .await
        .into_response();
    let put_body = to_bytes(put_response.into_body(), usize::MAX).await.unwrap();
    assert!(!String::from_utf8_lossy(&put_body).contains("SDCERR"));

    let delete_response = summit_rcm::plugins::unauthenticated::routes::v2::delete_unauthenticated()
        .await
        .into_response();
    let delete_body = to_bytes(delete_response.into_body(), usize::MAX).await.unwrap();
    assert!(!String::from_utf8_lossy(&delete_body).contains("SDCERR"));
}

#[cfg(all(feature = "radio-siso-mode", feature = "api-v2"))]
#[tokio::test]
async fn v2_radio_siso_invalid_payload_does_not_return_sdcerr() {
    use axum::{body::to_bytes, response::IntoResponse, Json};

    let response = summit_rcm::plugins::radio_siso_mode::routes::v2::put_radio_siso_mode(
        Json(summit_rcm::plugins::radio_siso_mode::routes::v2::SisoModePut {
            siso_mode: 999,
        }),
    )
    .await
    .into_response();

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_text = String::from_utf8_lossy(&body);
    assert!(!body_text.contains("SDCERR"));
    assert!(!body_text.contains("InfoMsg"));
}

#[cfg(all(feature = "api-docs", feature = "bluetooth", feature = "api-v2", feature = "api-legacy"))]
#[test]
fn bluetooth_device_get_openapi_matches_bluetooth_state_contract() {
    use serde_json::Value;

    let spec = serde_json::to_value(summit_rcm::web::openapi::build_openapi())
        .expect("OpenAPI document should serialize");

    let paths = spec
        .get("paths")
        .and_then(Value::as_object)
        .expect("OpenAPI document should contain paths");

    let v2_root_get = paths
        .get("/api/v2/bluetooth")
        .and_then(|path| path.get("get"))
        .expect("v2 bluetooth GET operation should be documented");
    let v2_device_get = paths
        .get("/api/v2/bluetooth/{controller}/{device}")
        .and_then(|path| path.get("get"))
        .expect("v2 bluetooth device GET operation should be documented");
    let legacy_root_get = paths
        .get("/bluetooth")
        .and_then(|path| path.get("get"))
        .expect("legacy bluetooth GET operation should be documented");
    let legacy_device_get = paths
        .get("/bluetooth/{controller}/{device}")
        .and_then(|path| path.get("get"))
        .expect("legacy bluetooth device GET operation should be documented");

    fn response_schema(operation: &Value) -> &Value {
        operation
            .get("responses")
            .and_then(|responses| responses.get("200"))
            .and_then(|response| response.get("content"))
            .and_then(|content| content.get("application/json"))
            .and_then(|media| media.get("schema"))
            .expect("GET operation should document a 200 JSON schema")
    }

    fn has_filter_query_param(operation: &Value) -> bool {
        operation
            .get("parameters")
            .and_then(Value::as_array)
            .is_some_and(|params| {
                params.iter().any(|param| {
                    param.get("in").and_then(Value::as_str) == Some("query")
                        && param.get("name").and_then(Value::as_str) == Some("filter")
                })
            })
    }

    assert_eq!(response_schema(v2_device_get), response_schema(v2_root_get));
    assert_eq!(response_schema(legacy_device_get), response_schema(legacy_root_get));
    assert!(has_filter_query_param(v2_device_get));
    assert!(has_filter_query_param(legacy_device_get));
}
