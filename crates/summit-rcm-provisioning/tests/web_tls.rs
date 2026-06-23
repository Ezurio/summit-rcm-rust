#![cfg(all(feature = "provisioning", feature = "api-v2"))]

use summit_rcm_core::config::test_support as config_test_support;
use summit_rcm_provisioning::{
    CertificateProvisioningService, ProvisioningState, ProvisioningWebTlsConfig,
};
use std::time::{SystemTime, UNIX_EPOCH};

macro_rules! test_env {
    () => {
        let _guard = config_test_support::SERVER_LOCK.lock();
        let _cleanup = ProvisioningTestCleanup;
        config_test_support::clear_server_overrides();
    };
    ($(($section:expr, $key:expr, $value:expr)),+ $(,)?) => {
        test_env!();
        $(config_test_support::set_server_override($section, $key, $value);)+
    };
}

struct ProvisioningTestCleanup;

impl Drop for ProvisioningTestCleanup {
    fn drop(&mut self) {
        config_test_support::clear_server_overrides();
        if let Some(state_path) = config_test_support::env_override("SUMMIT_RCM_PROVISIONING_STATE_FILE") {
            let _ = std::fs::remove_file(&state_path);
        }
        config_test_support::clear_env_override("SUMMIT_RCM_PROVISIONING_STATE_FILE");
        config_test_support::clear_env_override("SUMMIT_RCM_PROVISIONING_SERVER_CERT");
        config_test_support::clear_env_override("SUMMIT_RCM_PROVISIONING_SERVER_KEY");
    }
}

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

    config_test_support::set_env_override(
        "SUMMIT_RCM_PROVISIONING_SERVER_CERT",
        &cert_path.display().to_string(),
    );
    config_test_support::set_env_override(
        "SUMMIT_RCM_PROVISIONING_SERVER_KEY",
        &key_path.display().to_string(),
    );
}

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
        config_test_support::env_override("SUMMIT_RCM_PROVISIONING_SERVER_CERT")
            .expect("test provisioning cert override should be set")
    );
    assert_eq!(
        resolved.config.key_path,
        config_test_support::env_override("SUMMIT_RCM_PROVISIONING_SERVER_KEY")
            .expect("test provisioning key override should be set")
    );
    assert!(!resolved.config.require_client_auth);
    assert_eq!(resolved.mode_log, Some("*** RESTRICTED PROVISIONING MODE ***"));
}

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