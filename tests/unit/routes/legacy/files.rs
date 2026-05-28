use super::{put_files_legacy, validate_legacy_upload_request};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::put,
    Router,
};
use tower::ServiceExt;

#[test]
fn legacy_upload_rejects_missing_type_with_python_message() {
    let error = validate_legacy_upload_request("", "cert.pem", true).unwrap();
    assert_eq!(error.info_msg, "file POST - no type specified");
}

#[test]
fn legacy_upload_rejects_unknown_type_with_python_message() {
    let error = validate_legacy_upload_request("unknown", "data.bin", true).unwrap();
    assert_eq!(error.info_msg, "file POST type unknown unknown");
}

#[test]
fn legacy_upload_requires_zip_for_config() {
    let error = validate_legacy_upload_request("config", "data.txt", true).unwrap();
    assert_eq!(error.info_msg, "file POST type not .zip file");
}

#[test]
fn legacy_upload_requires_zip_for_timezone() {
    let error = validate_legacy_upload_request("timezone", "tz.bin", true).unwrap();
    assert_eq!(error.info_msg, "file POST type not .zip file");
}

#[test]
fn legacy_upload_accepts_zip_for_timezone() {
    assert!(validate_legacy_upload_request("timezone", "tz.zip", true).is_none());
}

#[tokio::test]
async fn legacy_files_put_missing_password_returns_http_200_with_invalid_password_body() {
    let boundary = "X-BOUNDARY";
    let multipart_body = concat!(
        "--X-BOUNDARY\r\n",
        "Content-Disposition: form-data; name=\"archive\"; filename=\"connections.zip\"\r\n",
        "Content-Type: application/zip\r\n",
        "\r\n",
        "not a real zip archive\r\n",
        "--X-BOUNDARY--\r\n",
    );

    let app = Router::new().route("/files", put(put_files_legacy));

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/files?type=network")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(multipart_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["SDCERR"], 1);
    assert_eq!(value["InfoMsg"], "Invalid password");
}