//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Runtime middleware for tracking the latest installed client cert and
//! propagating its `notBefore` into the fallback-timestamp file.
//!
//! Boot-mode-based route eviction (see [`RouteMode`]) handles "what is
//! reachable" decisions at router build time, so this layer no longer
//! gates routes; it only observes incoming TLS metadata.
//!
//! [`RouteMode`]: summit_rcm_plugin_api::RouteMode

use summit_rcm_core::config::ServerConfig;
use crate::service::{
    CertificateProvisioningService, ClientTlsInfo, ProvisioningState,
};
use summit_rcm_web::axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};
use std::sync::atomic::{AtomicU64, Ordering};
use log::warn;

fn disable_certificate_expiry_verification() -> bool {
    ServerConfig::get_bool("summit-rcm", "disable_certificate_expiry_verification", true)
}

/// 64-bit fingerprint of the last client cert seen. `0` is the unset
/// sentinel; a real cert that hashes to 0 is mapped to 1 to avoid colliding
/// with it. The worst case of a benign collision is one redundant
/// `set_fallback_timestamp` call, which is idempotent.
static LAST_CLIENT_CERT_FINGERPRINT: AtomicU64 = AtomicU64::new(0);

fn fingerprint_from_cert_hash(cert_hash: &str) -> u64 {
    let prefix = cert_hash.get(..16).unwrap_or(cert_hash);
    let value = u64::from_str_radix(prefix, 16).unwrap_or(0);
    if value == 0 { 1 } else { value }
}

async fn check_for_new_fallback_timestamp(tls_info: &ClientTlsInfo) {
    if disable_certificate_expiry_verification() {
        return;
    }

    if CertificateProvisioningService::get_provisioning_state_async().await
        != ProvisioningState::FullyProvisioned
    {
        return;
    }

    let cert_hash = match CertificateProvisioningService::get_client_cert_hash(tls_info) {
        Ok(cert_hash) => cert_hash,
        Err(_) => return,
    };

    let fingerprint = fingerprint_from_cert_hash(&cert_hash);
    if LAST_CLIENT_CERT_FINGERPRINT.swap(fingerprint, Ordering::Relaxed) == fingerprint {
        return;
    }

    let Ok(validity) =
        CertificateProvisioningService::get_client_cert_validity_period(tls_info)
    else {
        return;
    };

    let fallback_timestamp = CertificateProvisioningService::read_fallback_timestamp().ok().flatten();
    if fallback_timestamp.map(|ts| validity.not_before > ts).unwrap_or(true)
        && let Err(error) =
            CertificateProvisioningService::set_fallback_timestamp(validity.not_before).await
    {
        warn!("Couldn't update fallback timestamp from client certificate: {}", error);
    }
}

/// Observes the request's [`ClientTlsInfo`] and, when running in
/// `FullyProvisioned`, advances the on-disk fallback timestamp.
///
/// Applied only on routers built for fully-provisioned boot mode; in
/// provisioning boot mode this layer is omitted entirely.
pub async fn track_client_cert_fallback_timestamp(
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let tls_info = req
        .extensions()
        .get::<ClientTlsInfo>()
        .cloned()
        .unwrap_or_default();
    check_for_new_fallback_timestamp(&tls_info).await;
    next.run(req).await
}
