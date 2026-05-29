use super::*;
use crate::config::tests;
use crate::plugins::login::login_service::test_support;
#[cfg(feature = "provisioning")]
use crate::plugins::provisioning::service::{
    CertificateProvisioningService, ProvisioningState, ProvisioningWebTlsConfig,
};
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
        let _guard = tests::SERVER_LOCK.lock();
        let _cleanup = ServerConfigTestCleanup;
        tests::clear_server_overrides();
    };
    ($(($section:expr, $key:expr, $value:expr)),+ $(,)?) => {
        test_env!();
        $(tests::set_server_override($section, $key, $value);)+
    };
}

struct ServerConfigTestCleanup;

impl Drop for ServerConfigTestCleanup {
    fn drop(&mut self) {
        test_support::clear_test_state();
        tests::clear_server_overrides();
        tests::delete_system_setting("session_timeout");
        #[cfg(feature = "provisioning")]
        if let Ok(state_path) = std::env::var("SUMMIT_RCM_PROVISIONING_STATE_FILE") {
            let _ = std::fs::remove_file(&state_path);
            unsafe {
                std::env::remove_var("SUMMIT_RCM_PROVISIONING_STATE_FILE");
                std::env::remove_var("SUMMIT_RCM_PROVISIONING_SERVER_CERT");
                std::env::remove_var("SUMMIT_RCM_PROVISIONING_SERVER_KEY");
            }
        }
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

    unsafe {
        std::env::set_var(
            "SUMMIT_RCM_PROVISIONING_STATE_FILE",
            state_path.as_os_str(),
        );
    }

    tokio::fs::write(&state_path, format!("{}", state as i32))
        .await
        .expect("test provisioning state should be writable");
}

#[cfg(feature = "provisioning")]
async fn set_test_provisioning_tls_assets() {
    let base = std::env::temp_dir().join(format!(
        "summit-rcm-provisioning-tls-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos(),
    ));

    tokio::fs::create_dir_all(&base)
        .await
        .expect("test provisioning tls dir should be creatable");

    let cert_path = base.join("provisioning.crt");
    let key_path = base.join("provisioning.key");
    tokio::fs::write(&cert_path, "cert")
        .await
        .expect("test provisioning cert should be writable");
    tokio::fs::write(&key_path, "key")
        .await
        .expect("test provisioning key should be writable");

    unsafe {
        std::env::set_var("SUMMIT_RCM_PROVISIONING_SERVER_CERT", cert_path.as_os_str());
        std::env::set_var("SUMMIT_RCM_PROVISIONING_SERVER_KEY", key_path.as_os_str());
    }
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

#[test]
fn default_bind_addr_uses_configured_socket_port() {
    test_env!();

    assert_eq!(default_bind_addr(), "0.0.0.0:8080");

    tests::set_server_override("summit-rcm", "socket_port", "9443");
    assert_eq!(default_bind_addr(), "0.0.0.0:9443");
}

#[tokio::test]
async fn session_flow_requires_login_and_revokes_on_logout() {
    test_env!();

    let app: Router = build_router();

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
    let _settings_guard = tests::SETTINGS_LOCK.lock();
    test_env!();
    assert!(tests::set_system_setting("session_timeout", "1"));
    test_support::set_boottime_secs(100);

    let app: Router = build_router();

    let login_request = Request::builder()
        .method("POST")
        .uri("/api/v2/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let login_response = app.clone().oneshot(login_request).await.unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);
    let cookie = session_cookie(&login_response).expect("session cookie should be set");

    test_support::set_boottime_secs(161);

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

    let app: Router = build_router();

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

    let app: Router = build_router();

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
async fn sessions_disabled_missing_bluetooth_device_returns_not_found() {
    test_env!(("/", "tools.sessions.on", "false"));

    let app: Router = build_router();

    let request = Request::builder()
        .uri("/api/v2/bluetooth/controller0/00:00:00:00:00:00")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
fn at_command_publications_expose_feature_gated_metadata() {
    let commands: Vec<&str> = crate::publication::builtin_plugin_publications()
        .iter()
        .copied()
        .filter_map(|publication| publication.at_commands.as_ref())
        .flat_map(|publication| publication.install.iter().map(|(signature, _)| *signature))
        .collect();

    assert!(commands.contains(&"at+datetime"));
    assert!(commands.contains(&"at+connlist"));

    if cfg!(feature = "awm") {
        assert!(commands.contains(&"at+awmmode"));
        assert!(commands.contains(&"at+awmscan"));
    }

    if cfg!(feature = "log-forwarding") {
        assert!(commands.contains(&"at+logfwd"));
    }
}

#[cfg(feature = "at-interface")]
#[test]
fn at_lookup_resolves_core_and_plugin_usage_commands() {
    let (_, params, print_usage) = crate::at_interface::commands::lookup_command_in_registry("ATE1?")
        .expect("ATE1? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (_, params, print_usage) = crate::at_interface::commands::lookup_command_in_registry("AT+VER?")
        .expect("AT+VER? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);

    let (_, params, print_usage) = crate::at_interface::commands::lookup_command_in_registry("AT+DATETIME?")
        .expect("AT+DATETIME? should resolve");
    assert!(params.is_empty());
    assert!(print_usage);
}

#[cfg(feature = "provisioning")]
#[tokio::test]
async fn provisioning_tls_uses_provisioning_certificates_when_unprovisioned() {
    test_env!(("summit-rcm", "enable_client_pairing", "true"));
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;
    set_test_provisioning_tls_assets().await;

    let resolved = CertificateProvisioningService::resolve_web_tls_config(ProvisioningWebTlsConfig {
        cert_path: "/etc/summit-rcm/ssl/server.crt".to_string(),
        key_path: "/etc/summit-rcm/ssl/server.key".to_string(),
        ca_path: "/etc/summit-rcm/ssl/ca.crt".to_string(),
        require_client_auth: true,
    })
    .await
    .expect("provisioning TLS config should resolve");

    assert_eq!(
        resolved.config.cert_path,
        std::env::var("SUMMIT_RCM_PROVISIONING_SERVER_CERT")
            .unwrap_or_else(|_| crate::plugins::provisioning::PROVISIONING_SERVER_CERT_PATH.to_string())
            .trim_matches('"')
            .to_string()
    );
    assert_eq!(
        resolved.config.key_path,
        std::env::var("SUMMIT_RCM_PROVISIONING_SERVER_KEY")
            .unwrap_or_else(|_| crate::plugins::provisioning::PROVISIONING_SERVER_KEY_PATH.to_string())
            .trim_matches('"')
            .to_string()
    );
    assert!(!resolved.config.require_client_auth);
    assert_eq!(resolved.mode_log, Some("*** RESTRICTED PROVISIONING MODE ***"));
}

#[cfg(feature = "provisioning")]
#[tokio::test]
async fn provisioning_tls_falls_back_when_restricted_assets_are_missing() {
    test_env!(("summit-rcm", "enable_client_pairing", "true"));
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;

    let resolved = CertificateProvisioningService::resolve_web_tls_config(ProvisioningWebTlsConfig {
        cert_path: "/etc/summit-rcm/ssl/server.crt".to_string(),
        key_path: "/etc/summit-rcm/ssl/server.key".to_string(),
        ca_path: "/etc/summit-rcm/ssl/ca.crt".to_string(),
        require_client_auth: true,
    })
    .await
    .expect("missing restricted provisioning assets should not fail TLS config resolution");

    assert_eq!(resolved.config.cert_path, "/etc/summit-rcm/ssl/server.crt");
    assert_eq!(resolved.config.key_path, "/etc/summit-rcm/ssl/server.key");
    assert!(resolved.config.require_client_auth);
    assert_eq!(resolved.mode_log, None);
}

#[cfg(feature = "provisioning")]
#[tokio::test]
async fn provisioning_tls_requires_client_auth_when_pairing_is_enabled() {
    test_env!(("summit-rcm", "enable_client_pairing", "true"));
    set_test_provisioning_state(ProvisioningState::PartiallyProvisioned).await;

    let resolved = CertificateProvisioningService::resolve_web_tls_config(ProvisioningWebTlsConfig {
        cert_path: "/etc/summit-rcm/ssl/server.crt".to_string(),
        key_path: "/etc/summit-rcm/ssl/server.key".to_string(),
        ca_path: "/etc/summit-rcm/ssl/ca.crt".to_string(),
        require_client_auth: false,
    })
    .await
    .expect("provisioning TLS config should resolve");

    assert!(resolved.config.require_client_auth);
    assert_eq!(resolved.mode_log, Some("*** PARTIALLY PROVISIONED MODE ***"));
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
async fn provisioning_mode_router_omits_normal_login_routes() {
    test_env!(("summit-rcm", "enable_client_pairing", "true"));
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;

    let app: Router = build_router();

    let request = Request::builder()
        .method("POST")
        .uri("/api/v2/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"root","password":"summit"}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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