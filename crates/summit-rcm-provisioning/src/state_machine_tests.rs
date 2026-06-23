//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::{CertificateProvisioningService, ProvisioningState};
use crate::state_machine::{Event, ProvisioningStateMachine, TransitionError};
use summit_rcm_core::config::test_support as config_test_support;
use std::sync::MutexGuard;
use std::time::{SystemTime, UNIX_EPOCH};

struct ProvisioningStateMachineTestCleanup {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for ProvisioningStateMachineTestCleanup {
    fn drop(&mut self) {
        if let Some(state_path) = config_test_support::env_override("SUMMIT_RCM_PROVISIONING_STATE_FILE") {
            let _ = std::fs::remove_file(&state_path);
        }
        if let Some(device_cert_path) = config_test_support::env_override("SUMMIT_RCM_DEVICE_SERVER_CERT") {
            let _ = std::fs::remove_file(&device_cert_path);
        }
        config_test_support::clear_env_override("SUMMIT_RCM_PROVISIONING_STATE_FILE");
        config_test_support::clear_env_override("SUMMIT_RCM_DEVICE_SERVER_CERT");
        config_test_support::clear_env_override("SUMMIT_RCM_SKIP_PROVISIONING_RESTART");
        config_test_support::clear_server_overrides();
    }
}

fn test_env() -> ProvisioningStateMachineTestCleanup {
    let guard = config_test_support::SERVER_LOCK
        .lock()
        .expect("state machine tests should acquire config lock");
    config_test_support::clear_server_overrides();
    config_test_support::set_env_override("SUMMIT_RCM_SKIP_PROVISIONING_RESTART", "1");
    ProvisioningStateMachineTestCleanup { _guard: guard }
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

    CertificateProvisioningService::set_provisioning_state_async(state)
        .await
        .expect("test provisioning state should be writable");
}

async fn set_test_device_server_cert_path() -> std::path::PathBuf {
    let cert_path = std::env::temp_dir().join(format!(
        "summit-rcm-state-machine-device-cert-{}-{}.crt",
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
        .expect("openssl should run for provisioning state-machine test");
    assert!(status.success(), "openssl should generate a test certificate");
    let _ = tokio::fs::remove_file(key_path).await;
}

#[tokio::test]
async fn cert_upload_then_manual_time_set_completes_provisioning_flow() {
    let _cleanup = test_env();
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;
    let cert_path = set_test_device_server_cert_path().await;
    generate_test_device_certificate(&cert_path).await;

    ProvisioningStateMachine::handle(Event::CertUploaded)
        .await
        .expect("certificate upload transition should succeed");
    assert_eq!(ProvisioningStateMachine::current().await, ProvisioningState::PartiallyProvisioned);

    let validity = CertificateProvisioningService::get_device_server_cert_validity_period()
        .await
        .expect("generated device certificate validity should be readable");
    let inside = (validity.not_before.unix_timestamp() + 60) * 1_000_000;
    let before = (validity.not_before.unix_timestamp() - 60) * 1_000_000;

    assert!(
        CertificateProvisioningService::validate_new_timestamp(inside, &Default::default()).await,
        "manual time inside uploaded device cert window should be accepted"
    );
    assert!(
        !CertificateProvisioningService::validate_new_timestamp(before, &Default::default()).await,
        "manual time before uploaded device cert window should be rejected"
    );

    ProvisioningStateMachine::handle(Event::ManualTimeSet)
        .await
        .expect("manual time set transition should succeed");
    assert_eq!(ProvisioningStateMachine::current().await, ProvisioningState::FullyProvisioned);
}

#[tokio::test]
async fn wrong_state_events_are_rejected_or_noop_per_contract() {
    let _cleanup = test_env();
    set_test_provisioning_state(ProvisioningState::Unprovisioned).await;

    match ProvisioningStateMachine::handle(Event::ClientBundleUploaded).await {
        Err(TransitionError::WrongState { current, event: Event::ClientBundleUploaded }) => {
            assert_eq!(current, ProvisioningState::Unprovisioned);
        }
        other => panic!("unexpected client bundle result in Unprovisioned: {other:?}"),
    }

    ProvisioningStateMachine::handle(Event::ManualTimeSet)
        .await
        .expect("manual time set outside PartiallyProvisioned should be a no-op");
    assert_eq!(ProvisioningStateMachine::current().await, ProvisioningState::Unprovisioned);

    set_test_provisioning_state(ProvisioningState::PartiallyProvisioned).await;
    match ProvisioningStateMachine::handle(Event::CertUploaded).await {
        Err(TransitionError::WrongState { current, event: Event::CertUploaded }) => {
            assert_eq!(current, ProvisioningState::PartiallyProvisioned);
        }
        other => panic!("unexpected cert uploaded result in PartiallyProvisioned: {other:?}"),
    }

    ProvisioningStateMachine::handle(Event::ClientBundleUploaded)
        .await
        .expect("client bundle event should restart without changing state in PartiallyProvisioned");
    assert_eq!(ProvisioningStateMachine::current().await, ProvisioningState::PartiallyProvisioned);
}
