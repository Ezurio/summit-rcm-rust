#![cfg(all(feature = "login", feature = "test-support"))]

//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Application-level integration tests.
//!
//! These exercise the fully assembled router through the root application
//! crate, so the test target must call `summit_rcm::web::build_router()` and
//! rely on the root crate's `plugin_links.rs` link-retention shim.

use summit_rcm_core::config::test_support as config_test_support;
use summit_rcm_login::test_support as login_test_support;
use summit_rcm::web::build_router;
#[cfg(feature = "provisioning")]
use summit_rcm_provisioning::ProvisioningState;
#[cfg(feature = "provisioning")]
use std::time::{SystemTime, UNIX_EPOCH};
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use tower::ServiceExt;

macro_rules! test_env {
    () => {
        let _guard = config_test_support::SERVER_LOCK.lock();
        let _cleanup = ServerConfigTestCleanup;
        config_test_support::clear_server_overrides();
    };
    ($(($section:expr, $key:expr, $value:expr)),+ $(,)?) => {
        test_env!();
        $(config_test_support::set_server_override($section, $key, $value);)+
    };
}

struct ServerConfigTestCleanup;

impl Drop for ServerConfigTestCleanup {
    fn drop(&mut self) {
        login_test_support::clear_test_state();
        config_test_support::clear_server_overrides();
        config_test_support::delete_system_setting("session_timeout");
        #[cfg(feature = "provisioning")]
        if let Some(state_path) = config_test_support::env_override("SUMMIT_RCM_PROVISIONING_STATE_FILE") {
            let _ = std::fs::remove_file(&state_path);
        }
        #[cfg(feature = "provisioning")]
        if let Some(device_cert_path) = config_test_support::env_override("SUMMIT_RCM_DEVICE_SERVER_CERT") {
            let _ = std::fs::remove_file(&device_cert_path);
        }
        #[cfg(feature = "provisioning")]
        config_test_support::clear_env_override("SUMMIT_RCM_PROVISIONING_STATE_FILE");
        #[cfg(feature = "provisioning")]
        config_test_support::clear_env_override("SUMMIT_RCM_DEVICE_SERVER_CERT");
        #[cfg(feature = "provisioning")]
        config_test_support::clear_env_override("SUMMIT_RCM_PROVISIONING_SERVER_CERT");
        #[cfg(feature = "provisioning")]
        config_test_support::clear_env_override("SUMMIT_RCM_PROVISIONING_SERVER_KEY");
    }
}

#[cfg(feature = "provisioning")]
async fn set_test_provisioning_state(state: ProvisioningState) {
    let state_path = std::env::temp_dir().join(format!(
        "summit-rcm-provisioning-state-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos(),
    ));

    config_test_support::set_env_override(
        "SUMMIT_RCM_PROVISIONING_STATE_FILE",
        &state_path.display().to_string(),
    );

    tokio::fs::write(&state_path, format!("{}", state as i32))
        .await
        .expect("test provisioning state should be writable");
}

#[cfg(feature = "provisioning")]
async fn set_test_device_server_cert_path() -> std::path::PathBuf {
    let cert_path = std::env::temp_dir().join(format!(
        "summit-rcm-app-device-cert-{}-{}.crt",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos(),
    ));

    config_test_support::set_env_override(
        "SUMMIT_RCM_DEVICE_SERVER_CERT",
        &cert_path.display().to_string(),
    );

    cert_path
}

#[cfg(feature = "provisioning")]
async fn generate_test_device_certificate(cert_path: &std::path::Path) {
    let key_path = cert_path.with_extension("key");
    let status = tokio::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-nodes",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:prime256v1",
            "-pkeyopt",
            "ec_param_enc:named_curve",
            "-subj",
            "/CN=RcmDevice",
            "-set_serial",
            "1",
            "-days",
            "2",
            "-keyout",
        ])
        .arg(&key_path)
        .args(["-out"])
        .arg(cert_path)
        .status()
        .await
        .expect("openssl should run for app provisioning test");
    assert!(status.success(), "openssl should generate a test certificate");
    let _ = tokio::fs::remove_file(key_path).await;
}

fn session_cookie(response: &axum::response::Response) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("session_id="))
        .map(|value| value.split(';').next().unwrap_or_default().to_string())
}

fn session_cookie_set_cookie_header(response: &axum::response::Response) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("session_id="))
        .map(|value| value.to_string())
}

#[tokio::test]
async fn session_flow_requires_login_and_revokes_on_logout() {
    test_env!();

    let app: Router = build_router().layer(axum::Extension(
        summit_rcm_provisioning::ClientTlsInfo::default(),
    ));

    let request = Request::builder()
        .uri("/api/v2/login/users")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v2/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = session_cookie(&response).expect("session cookie should be set");

    let request = Request::builder()
        .uri("/api/v2/login/users")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("DELETE")
        .uri("/api/v2/login")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/api/v2/login/users")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_session_loses_access_to_protected_route() {
    let _settings_guard = config_test_support::SETTINGS_LOCK.lock();
    test_env!();
    assert!(config_test_support::set_system_setting("session_timeout", "1"));
    login_test_support::set_boottime_secs(100);

    let app: Router = build_router().layer(axum::Extension(
        summit_rcm_provisioning::ClientTlsInfo::default(),
    ));

    let login_request = Request::builder()
        .method("POST")
        .uri("/api/v2/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let login_response = app.clone().oneshot(login_request).await.unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);
    let cookie = session_cookie(&login_response).expect("session cookie should be set");

    login_test_support::set_boottime_secs(161);

    let request = Request::builder()
        .uri("/api/v2/login/users")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unauthenticated_reset_reboot_setting_routes_require_login() {
    test_env!();

    let app: Router = build_router().layer(axum::Extension(
        summit_rcm_provisioning::ClientTlsInfo::default(),
    ));

    let request = Request::builder()
        .uri("/api/v2/system/allowUnauthenticatedResetReboot")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .uri("/allowUnauthenticatedResetReboot")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sessions_disabled_allows_protected_v2_route_without_login() {
    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router().layer(axum::Extension(
        summit_rcm_provisioning::ClientTlsInfo::default(),
    ));

    let request = Request::builder()
        .uri("/api/v2/login/users")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("DELETE")
        .uri("/api/v2/login")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn logout_without_session_returns_bad_request_when_sessions_enabled() {
    test_env!();

    let app: Router = build_router();

    let request = Request::builder()
        .method("DELETE")
        .uri("/api/v2/login")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sessions_disabled_create_duplicate_user_returns_conflict() {
    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .method("POST")
        .uri("/api/v2/login/users")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"ignored","permissions":"system_user"}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[cfg(feature = "bluetooth")]
#[tokio::test]
async fn sessions_disabled_missing_bluetooth_device_returns_bad_request() {
    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .uri("/api/v2/bluetooth/controller0/00:00:00:00:00:00")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn session_cookie_uses_configured_secure_and_httponly_flags() {
    test_env!(("/", "tools.sessions.secure", "false"), ("/", "tools.sessions.httponly", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .method("POST")
        .uri("/api/v2/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let cookie = session_cookie_set_cookie_header(&response)
        .expect("session cookie should be set");
    assert!(!cookie.contains("Secure"));
    assert!(!cookie.contains("HttpOnly"));
}

#[tokio::test]
async fn session_cookie_defaults_to_secure_and_httponly() {
    test_env!();

    let app: Router = build_router();

    let request = Request::builder()
        .method("POST")
        .uri("/api/v2/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let cookie = session_cookie_set_cookie_header(&response)
        .expect("session cookie should be set");
    assert!(cookie.contains("Secure"));
    assert!(cookie.contains("HttpOnly"));
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_logout_requires_login_and_returns_ok_after_login() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!();

    let app: Router = build_router();

    let request = Request::builder()
        .method("DELETE")
        .uri("/login")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["InfoMsg"], "user not found");

    let request = Request::builder()
        .method("POST")
        .uri("/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = session_cookie(&response).expect("session cookie should be set");

    let request = Request::builder()
        .method("DELETE")
        .uri("/login")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["InfoMsg"], "user root logged out");
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_delete_user_uses_path_param_shape() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!();

    let app: Router = build_router();

    let login_request = Request::builder()
        .method("POST")
        .uri("/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let login_response = app.clone().oneshot(login_request).await.unwrap();
    let cookie = session_cookie(&login_response).expect("session cookie should be set");

    let request = Request::builder()
        .method("DELETE")
        .uri("/users/root")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["InfoMsg"], "unable to remove root user");
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_delete_connection_uses_path_param_shape() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .method("DELETE")
        .uri("/connection/not-a-real-uuid")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["SDCERR"], 1);
    assert_eq!(body["InfoMsg"], "Unable to delete connection, not found");
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_wifi_enable_rejects_invalid_enable_query() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .method("PUT")
        .uri("/wifiEnable?enable=bad")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["SDCERR"], 1);
    assert_eq!(
        body["InfoMsg"],
        "unable to set wireless_set_enable. Supplied enable parameter 'bad' invalid."
    );
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_activate_connection_rejects_missing_uuid_profile() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .method("PUT")
        .uri("/connection")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"uuid":"not-a-real-uuid","activate":1}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["SDCERR"], 1);
    assert_eq!(body["InfoMsg"], "UUID not found");
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_version_keeps_standard_response_envelope() {
    use axum::body::to_bytes;
    use serde_json::Value;

    let app: Router = build_router();

    let request = Request::builder().uri("/version").body(Body::empty()).unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: Value = serde_json::from_slice(&body).unwrap();

    assert!(payload.get("SDCERR").is_some());
    assert!(payload.get("InfoMsg").is_some());
    assert!(payload["InfoMsg"].is_string());
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_log_setting_requires_supp_debug_level_with_python_message() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .method("POST")
        .uri("/logSetting")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"driverDebugLevel":0}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["SDCERR"], 1);
    assert_eq!(body["InfoMsg"], "suppDebugLevel missing from JSON data");
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_log_setting_requires_driver_debug_level_with_python_message() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .method("POST")
        .uri("/logSetting")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"suppDebugLevel":"info"}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["SDCERR"], 1);
    assert_eq!(body["InfoMsg"], "driverDebugLevel missing from JSON data");
}

#[cfg(feature = "api-legacy")]
#[tokio::test]
async fn legacy_log_data_rejects_out_of_range_priority_with_python_message() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .uri("/logData?priority=9")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["SDCERR"], 1);
    assert_eq!(body["InfoMsg"], "Priority must be an int between 0-7");
}

#[cfg(feature = "at-interface")]
#[test]
fn at_lookup_resolves_core_and_plugin_usage_commands() {
    let (_, params, print_usage) = summit_rcm_at::commands::lookup_command_in_registry("ATE1?")
        .expect("ATE1? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (_, params, print_usage) = summit_rcm_at::commands::lookup_command_in_registry("AT+VER?")
        .expect("AT+VER? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (_, params, print_usage) = summit_rcm_at::commands::lookup_command_in_registry("AT+DATETIME?")
        .expect("AT+DATETIME? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);
}

#[cfg(feature = "provisioning")]
#[tokio::test]
async fn provisioning_mode_router_exposes_provisioning_routes_without_login() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(("summit-rcm", "enable_client_pairing", "true"));
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;

    let app: Router = build_router();

    let request = Request::builder()
        .uri("/api/v2/system/certificateProvisioning")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["state"], 0);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v2/system/datetime")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[cfg(feature = "provisioning")]
#[tokio::test]
async fn provisioning_mode_router_exposes_public_login_but_omits_user_management_routes() {
    test_env!(("summit-rcm", "enable_client_pairing", "true"));
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;

    let app: Router = build_router();

    let request = Request::builder()
        .method("POST")
        .uri("/api/v2/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/api/v2/login/users")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[cfg(feature = "provisioning")]
#[tokio::test]
async fn fully_provisioned_router_exposes_provisioning_state_route_and_normal_datetime() {
    test_env!(
        ("summit-rcm", "enable_client_pairing", "true"),
        ("/", "tools.sessions.on", "false")
    );
    set_test_provisioning_state(ProvisioningState::FullyProvisioned).await;

    let app: Router = build_router();

    let request = Request::builder()
        .uri("/api/v2/system/certificateProvisioning")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/api/v2/system/datetime")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[cfg(all(feature = "provisioning", feature = "api-legacy"))]
#[tokio::test]
async fn fully_provisioned_router_exposes_legacy_provisioning_routes() {
    test_env!(
        ("summit-rcm", "enable_client_pairing", "true"),
        ("/", "tools.sessions.on", "false")
    );
    set_test_provisioning_state(ProvisioningState::FullyProvisioned).await;

    let app: Router = build_router();

    let request = Request::builder()
        .uri("/certificateProvisioning")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[cfg(feature = "provisioning")]
#[tokio::test]
async fn partially_provisioned_router_exposes_login_and_uses_provisioning_datetime_override() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(
        ("summit-rcm", "enable_client_pairing", "true"),
        ("/", "tools.sessions.on", "false")
    );
    set_test_provisioning_state(ProvisioningState::PartiallyProvisioned).await;
    let cert_path = set_test_device_server_cert_path().await;
    generate_test_device_certificate(&cert_path).await;

    let app: Router = build_router().layer(axum::Extension(
        summit_rcm_provisioning::ClientTlsInfo::default(),
    ));

    let request = Request::builder()
        .method("POST")
        .uri("/api/v2/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("PUT")
        .uri("/api/v2/system/datetime")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"datetime":"1"}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert!(body["notBefore"].is_string());
    assert!(body["notAfter"].is_string());
}

#[cfg(all(feature = "provisioning", feature = "api-legacy"))]
#[tokio::test]
async fn legacy_provisioning_post_invalid_upload_returns_bad_request() {
    test_env!(
        ("summit-rcm", "enable_client_pairing", "true"),
        ("/", "tools.sessions.on", "false")
    );
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;

    let app: Router = build_router();

    let request = Request::builder()
        .method("POST")
        .uri("/certificateProvisioning")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"dummy":true}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[cfg(all(feature = "provisioning", feature = "api-legacy"))]
#[tokio::test]
async fn legacy_provisioning_put_already_provisioned_matches_python_response() {
    use axum::body::to_bytes;
    use serde_json::Value;

    test_env!(
        ("summit-rcm", "enable_client_pairing", "true"),
        ("/", "tools.sessions.on", "false")
    );
    set_test_provisioning_state(ProvisioningState::PartiallyProvisioned).await;

    let app: Router = build_router();

    let request = Request::builder()
        .method("PUT")
        .uri("/certificateProvisioning")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"dummy":true}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["SDCERR"], 1);
    assert_eq!(body["InfoMsg"], "Already provisioned");
}
