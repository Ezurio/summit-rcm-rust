//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::config::ServerConfig;
use crate::definition::SUMMIT_RCM_TIME_FORMAT_DESCRIPTION;
use crate::plugins::provisioning::routes::common::{
    TimestampValidityPayload, invalid_timestamp_response,
};
use crate::plugins::provisioning::enable_client_pairing;
use crate::plugins::provisioning::service::{
    CertificateProvisioningService, ClientTlsInfo, ProvisioningState,
};
use axum::{
    body::{to_bytes, Body},
    extract::Request,
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use parking_lot::Mutex;
use serde::Deserialize;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use time::{OffsetDateTime, UtcDateTime};
use log::warn;

#[cfg(not(test))]
static DISABLE_CERTIFICATE_EXPIRY_VERIFICATION: AtomicBool = AtomicBool::new(true);

pub(crate) fn initialize_disable_certificate_expiry_verification() {
    #[cfg(not(test))]
    DISABLE_CERTIFICATE_EXPIRY_VERIFICATION.store(
        ServerConfig::get_bool("summit-rcm", "disable_certificate_expiry_verification", true),
        Ordering::Relaxed,
    );
}

fn rest_api_docs_enabled() -> bool {
    std::env::var("DOCS_GENERATION")
        .map(|value| value == "True")
        .unwrap_or(false)
}

fn disable_certificate_expiry_verification() -> bool {
    #[cfg(test)]
    {
        return ServerConfig::get_bool("summit-rcm", "disable_certificate_expiry_verification", true);
    }

    #[cfg(not(test))]
    {
        DISABLE_CERTIFICATE_EXPIRY_VERIFICATION.load(Ordering::Relaxed)
    }
}

static LAST_CLIENT_CERT_HASH: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

static UNPROVISIONED_PATH_WHITE_LIST: &[&str] = &[
    "/datetime",
    "/api/v2/system/datetime",
    "/api/v2/system/provisioning",
    "/api/v2/system/clientBundle",
    "/api/v2/network/status",
    "/api/v2/system/version",
    "/api/v2/system/power",
];

#[derive(Clone, Copy, Debug, Default)]
pub struct ProvisioningAuthOverride;

#[derive(Deserialize, Default)]
struct LegacyDateTimeBody {
    zone: Option<String>,
    method: Option<String>,
    datetime: Option<String>,
}

#[derive(Deserialize, Default)]
struct V2DateTimeBody {
    zone: Option<String>,
    timezone: Option<String>,
    datetime: Option<String>,
}

fn is_whitelisted(path: &str) -> bool {
    if UNPROVISIONED_PATH_WHITE_LIST.contains(&path) {
        return true;
    }

    if rest_api_docs_enabled() {
        return path == "/" || path.starts_with("/swagger-ui") || path == "/api-docs/openapi.json";
    }

    false
}

fn validity_payload(tls_info: &ClientTlsInfo) -> TimestampValidityPayload {
    let mut payload = TimestampValidityPayload {
        time: format_now_utc(),
        not_before: None,
        not_after: None,
    };

    if let Ok((not_before, not_after)) = CertificateProvisioningService::get_validity_period(tls_info) {
        payload.not_before = Some(format_utc_datetime(not_before));
        payload.not_after = Some(format_utc_datetime(not_after));
    }

    payload
}

fn format_now_utc() -> String {
    OffsetDateTime::now_utc()
        .format(SUMMIT_RCM_TIME_FORMAT_DESCRIPTION)
        .unwrap_or_else(|now| now.to_string())
}

fn format_utc_datetime(datetime: UtcDateTime) -> String {
    datetime
        .format(SUMMIT_RCM_TIME_FORMAT_DESCRIPTION)
        .unwrap_or_else(|datetime| datetime.to_string())
}

fn parse_requested_timestamp(path: &str, body: &[u8]) -> Option<i64> {
    if body.is_empty() {
        return None;
    }

    if path == "/datetime" {
        let parsed: LegacyDateTimeBody = serde_json::from_slice(body).ok()?;
        let zone = parsed.zone.unwrap_or_default();
        let method = parsed.method.unwrap_or_default();
        let datetime = parsed.datetime.unwrap_or_default();
        if zone.is_empty() && method == "manual" && !datetime.is_empty() {
            return datetime.parse::<i64>().ok();
        }
        return None;
    }

    if path == "/api/v2/system/datetime" {
        let parsed: V2DateTimeBody = serde_json::from_slice(body).ok()?;
        let zone = parsed.zone.or(parsed.timezone).unwrap_or_default();
        let datetime = parsed.datetime.unwrap_or_default();
        if zone.is_empty() && !datetime.is_empty() {
            return datetime.parse::<i64>().ok();
        }
    }

    None
}

fn check_for_new_fallback_timestamp(provisioning_state: ProvisioningState, tls_info: &ClientTlsInfo) {
    if disable_certificate_expiry_verification() {
        return;
    }

    if provisioning_state != ProvisioningState::FullyProvisioned {
        return;
    }

    let cert_hash = match CertificateProvisioningService::get_client_cert_hash(tls_info) {
        Ok(cert_hash) => cert_hash,
        Err(_) => return,
    };

    {
        let mut last_hash = LAST_CLIENT_CERT_HASH.lock();
        if last_hash.as_ref() == Some(&cert_hash) {
            return;
        }
        *last_hash = Some(cert_hash);
    }

    let Ok((client_cert_not_before, _)) = CertificateProvisioningService::get_client_cert_validity_period(tls_info) else {
        return;
    };

    let fallback_timestamp = CertificateProvisioningService::read_fallback_timestamp().ok().flatten();
    if fallback_timestamp.map(|ts| client_cert_not_before > ts).unwrap_or(true) {
        if let Err(error) = CertificateProvisioningService::set_fallback_timestamp(client_cert_not_before) {
            warn!("Couldn't update fallback timestamp from client certificate: {}", error);
        }
    }
}

pub async fn require_provisioning(mut req: Request<Body>, next: Next) -> Response<Body> {
    let provisioning_state = CertificateProvisioningService::get_provisioning_state();
    let enable_client_pairing = enable_client_pairing();
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let tls_info = req
        .extensions()
        .get::<ClientTlsInfo>()
        .cloned()
        .unwrap_or_default();

    check_for_new_fallback_timestamp(provisioning_state, &tls_info);

    let mut manual_time_set_request = false;
    if !disable_certificate_expiry_verification()
        && method == Method::PUT
        && (path == "/datetime" || path == "/api/v2/system/datetime")
    {
        let (parts, body) = req.into_parts();
        let body = match to_bytes(body, 64 * 1024).await {
            Ok(body) => body,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        };

        if let Some(new_timestamp_usec) = parse_requested_timestamp(&path, &body) {
            manual_time_set_request = true;
            if !CertificateProvisioningService::validate_new_timestamp(new_timestamp_usec, &tls_info) {
                let payload = validity_payload(&tls_info);
                return invalid_timestamp_response(&path, payload).into_response();
            }
        }

        req = Request::from_parts(parts, Body::from(body));
    }

    if provisioning_state != ProvisioningState::FullyProvisioned {
        if enable_client_pairing && provisioning_state == ProvisioningState::PartiallyProvisioned {
            req.extensions_mut().insert(ProvisioningAuthOverride);
        } else if is_whitelisted(&path) {
            req.extensions_mut().insert(ProvisioningAuthOverride);
        } else {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }

    let response = next.run(req).await;

    if manual_time_set_request
        && response.status().is_success()
        && provisioning_state == ProvisioningState::PartiallyProvisioned
    {
        if CertificateProvisioningService::set_provisioning_state(ProvisioningState::FullyProvisioned).is_ok() {
            tokio::spawn(async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let _ = CertificateProvisioningService::restart_summit_rcm().await;
            });
        }
    }

    response
}