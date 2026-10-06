#![cfg(all(feature = "provisioning", feature = "api-v2"))]

use std::time::{SystemTime, UNIX_EPOCH};
use summit_rcm_core::config::test_support as config_test_support;
use summit_rcm_provisioning::{CertificateProvisioningService, ClientTlsInfo, ProvisioningState};
use time::macros::format_description;

macro_rules! test_env {
    () => {
        let _guard = config_test_support::SERVER_LOCK.lock();
        let _cleanup = ProvisioningOperationTestCleanup;
        config_test_support::clear_server_overrides();
    };
}

struct ProvisioningOperationTestCleanup;

impl Drop for ProvisioningOperationTestCleanup {
    fn drop(&mut self) {
        config_test_support::clear_server_overrides();
        if let Some(state_path) =
            config_test_support::env_override("SUMMIT_RCM_PROVISIONING_STATE_FILE")
        {
            let _ = std::fs::remove_file(&state_path);
        }
        if let Some(device_cert_path) =
            config_test_support::env_override("SUMMIT_RCM_DEVICE_SERVER_CERT")
        {
            let _ = std::fs::remove_file(&device_cert_path);
        }
        config_test_support::clear_env_override("SUMMIT_RCM_PROVISIONING_STATE_FILE");
        config_test_support::clear_env_override("SUMMIT_RCM_DEVICE_SERVER_CERT");
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

async fn set_test_device_server_cert_path() -> std::path::PathBuf {
    let cert_path = std::env::temp_dir().join(format!(
        "summit-rcm-device-cert-{}-{}.crt",
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

fn temp_cert_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "summit-rcm-{label}-{}-{}.crt",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos(),
    ))
}

fn test_tls_info() -> ClientTlsInfo {
    ClientTlsInfo::default()
}

async fn generate_test_certificate(cert_path: &std::path::Path, common_name: &str, days: u32) {
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
        ])
        .arg(format!("/CN={common_name}"))
        .args(["-set_serial", "1", "-days"])
        .arg(days.to_string())
        .args(["-keyout"])
        .arg(&key_path)
        .args(["-out"])
        .arg(cert_path)
        .status()
        .await
        .expect("openssl should run for provisioning operation test");
    assert!(
        status.success(),
        "openssl should generate a test certificate"
    );
    let _ = tokio::fs::remove_file(key_path).await;
}

async fn generate_test_device_certificate(cert_path: &std::path::Path) {
    generate_test_certificate(cert_path, "RcmDevice", 2).await;
}

fn timestamp_usec_days_from_now(days: i64) -> i64 {
    (time::OffsetDateTime::now_utc().unix_timestamp() + (days * 86_400)) * 1_000_000
}

fn assert_validity_string(value: Option<&str>) {
    let value = value.expect("validity field should be present");
    let _parsed = time::PrimitiveDateTime::parse(
        value,
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
    )
    .expect("validity field should match summit timestamp format");
}

#[tokio::test]
async fn partially_provisioned_uses_uploaded_device_cert_validity_window() {
    test_env!();
    set_test_provisioning_state(ProvisioningState::PartiallyProvisioned).await;
    let cert_path = set_test_device_server_cert_path().await;
    generate_test_device_certificate(&cert_path).await;

    let payload =
        CertificateProvisioningService::timestamp_validity_payload(&test_tls_info()).await;

    assert_validity_string(payload.not_before.as_deref());
    assert_validity_string(payload.not_after.as_deref());

    let not_before = time::PrimitiveDateTime::parse(
        payload
            .not_before
            .as_deref()
            .expect("notBefore should be present"),
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
    )
    .expect("notBefore should parse")
    .assume_utc()
    .unix_timestamp();
    let inside = (not_before + 60) * 1_000_000;
    let before = (not_before - 60) * 1_000_000;

    assert!(CertificateProvisioningService::validate_new_timestamp(inside, &test_tls_info()).await);
    assert!(
        !CertificateProvisioningService::validate_new_timestamp(before, &test_tls_info()).await
    );
}

#[tokio::test]
async fn unprovisioned_without_uploaded_device_cert_has_no_validity_payload() {
    test_env!();
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;

    let payload =
        CertificateProvisioningService::timestamp_validity_payload(&test_tls_info()).await;

    assert!(payload.not_before.is_none());
    assert!(payload.not_after.is_none());
}

#[tokio::test]
async fn partially_provisioned_prefers_uploaded_device_cert_over_ca_window() {
    test_env!();
    set_test_provisioning_state(ProvisioningState::PartiallyProvisioned).await;

    let device_cert_path = set_test_device_server_cert_path().await;
    generate_test_certificate(&device_cert_path, "RcmDevice", 2).await;

    let ca_cert_path = temp_cert_path("ca-cert");
    generate_test_certificate(&ca_cert_path, "ProvisioningCa", 365).await;
    config_test_support::set_server_override(
        "global",
        "server.ssl_certificate_chain",
        &ca_cert_path.display().to_string(),
    );

    let inside_ca_but_outside_device = timestamp_usec_days_from_now(3);

    assert!(
        !CertificateProvisioningService::validate_new_timestamp(
            inside_ca_but_outside_device,
            &test_tls_info(),
        )
        .await,
        "partially provisioned validation should be driven by the uploaded device cert window",
    );

    let _ = tokio::fs::remove_file(ca_cert_path).await;
}

#[tokio::test]
async fn unprovisioned_uses_ca_window_even_if_device_cert_exists() {
    test_env!();
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;

    let device_cert_path = set_test_device_server_cert_path().await;
    generate_test_certificate(&device_cert_path, "RcmDevice", 2).await;

    let ca_cert_path = temp_cert_path("ca-cert");
    generate_test_certificate(&ca_cert_path, "ProvisioningCa", 365).await;
    config_test_support::set_server_override(
        "global",
        "server.ssl_certificate_chain",
        &ca_cert_path.display().to_string(),
    );

    let inside_ca_but_outside_device = timestamp_usec_days_from_now(3);

    assert!(
        CertificateProvisioningService::validate_new_timestamp(
            inside_ca_but_outside_device,
            &test_tls_info(),
        )
        .await,
        "unprovisioned validation should still fall back to the CA window before cert upload state is reached",
    );

    let _ = tokio::fs::remove_file(ca_cert_path).await;
}
