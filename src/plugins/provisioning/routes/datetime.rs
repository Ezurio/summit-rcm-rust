//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Provisioning-mode shadow handlers for `/api/v2/system/datetime` and
//! `/datetime`.
//!
//! These replace the normal `date_time` plugin's PUT handlers when the daemon
//! boots into provisioning mode (`Unprovisioned` / `PartiallyProvisioned`).
//! Build-time eviction (driven by [`RouteMode`]) ensures only one set of
//! handlers is registered per boot.
//!
//! Provisioning semantics added on top of the plain datetime setter:
//!   * Manual timestamps are validated against the installed device server
//!     certificate's notBefore (see [`CertificateProvisioningService`]).
//!   * A successful manual set fires [`Event::ManualTimeSet`], which the
//!     state machine uses to advance `PartiallyProvisioned -> FullyProvisioned`.
//!
//! [`RouteMode`]: summit_rcm_plugin_api::RouteMode
//! [`CertificateProvisioningService`]: crate::plugins::provisioning::service::CertificateProvisioningService
//! [`Event::ManualTimeSet`]: crate::plugins::provisioning::state_machine::Event::ManualTimeSet

use axum::Json;
use axum::extract::Extension;
use log::error;

use crate::dbus;
use crate::plugins::date_time::service::DateTimeService;
use crate::plugins::provisioning::service::{CertificateProvisioningService, ClientTlsInfo};
use crate::plugins::provisioning::state_machine::{Event, ProvisioningStateMachine};

#[cfg(feature = "api-v2")]
use crate::plugins::date_time::routes::v2::{DateTimeInfo, DateTimeRequest, SetDateTimeResponses};

#[cfg(feature = "api-legacy")]
use crate::plugins::date_time::routes::legacy::{
    DateTimeBody, LegacyDateTimeResponse, PutDateTimeLegacyResponses,
};
#[cfg(feature = "api-legacy")]
use crate::web::legacy_response::{fail_response, ok_response};

// GET semantics are identical in both boot modes; reuse the date_time
// handlers directly so the provisioning router owns the full path surface.
#[cfg(feature = "api-v2")]
pub use crate::plugins::date_time::routes::v2::get_datetime;
#[cfg(feature = "api-legacy")]
pub use crate::plugins::date_time::routes::legacy::get_datetime_legacy;

#[cfg(feature = "api-v2")]
fn invalid_timestamp_info() -> DateTimeInfo {
    DateTimeInfo {
        zones: vec![],
        zone: String::new(),
        datetime: DateTimeService::now_utc_formatted(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/datetime",
    tag = "system",
    request_body = DateTimeRequest,
    responses(SetDateTimeResponses)
))]
#[cfg(feature = "api-v2")]
pub async fn put_datetime(
    Extension(tls_info): Extension<ClientTlsInfo>,
    Json(body): Json<DateTimeRequest>,
) -> SetDateTimeResponses {
    let zone = body.zone.or(body.timezone);
    if let Some(tz) = zone.clone()
        && let Err(e) = DateTimeService::set_timezone(&tz).await {
            error!("set_datetime timezone: {}", e);
            return if dbus::is_timeout_error(&e) {
                SetDateTimeResponses::Timeout
            } else {
                SetDateTimeResponses::InternalError
            };
        }

    let mut manual_time_set_request = false;
    if let Some(datetime) = body.datetime {
        if zone.as_deref().unwrap_or_default().is_empty() {
            manual_time_set_request = true;
            if let Ok(parsed) = datetime.parse::<i64>()
                && !CertificateProvisioningService::validate_new_timestamp(parsed, &tls_info).await
            {
                return SetDateTimeResponses::BadRequest(invalid_timestamp_info());
            }
        }

        if let Err(e) = DateTimeService::set_time_manual(&datetime).await {
            error!("set_datetime manual: {}", e);
            return if dbus::is_timeout_error(&e) {
                SetDateTimeResponses::Timeout
            } else {
                SetDateTimeResponses::InternalError
            };
        }
    }

    if manual_time_set_request {
        let _ = ProvisioningStateMachine::handle(Event::ManualTimeSet).await;
    }

    match DateTimeService::list_timezones().await {
        Ok(zones) => {
            let zone = DateTimeService::local_zone().await;
            let v = DateTimeService::get_datetime(zones, zone);
            SetDateTimeResponses::Ok(DateTimeInfo {
                zones: v.zones,
                zone: v.zone,
                datetime: v.datetime,
            })
        }
        Err(e) => {
            error!("set_datetime get: {}", e);
            if dbus::is_timeout_error(&e) {
                SetDateTimeResponses::Timeout
            } else {
                SetDateTimeResponses::InternalError
            }
        }
    }
}

#[cfg(feature = "api-legacy")]
fn invalid_timestamp_legacy_response() -> LegacyDateTimeResponse {
    LegacyDateTimeResponse {
        operation: fail_response("Invalid timestamp"),
        zones: None,
        zone: None,
        method: None,
        time: DateTimeService::now_utc_formatted(),
    }
}

#[cfg(feature = "api-legacy")]
fn legacy_put_response(
    operation: crate::web::legacy_response::LegacyOperationResponse,
    time: impl Into<String>,
) -> LegacyDateTimeResponse {
    LegacyDateTimeResponse {
        operation,
        zones: None,
        zone: None,
        method: None,
        time: time.into(),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/datetime",
    tag = "legacy",
    request_body = DateTimeBody,
    responses(PutDateTimeLegacyResponses)
))]
#[cfg(feature = "api-legacy")]
pub async fn put_datetime_legacy(
    Extension(tls_info): Extension<ClientTlsInfo>,
    Json(body): Json<DateTimeBody>,
) -> PutDateTimeLegacyResponses {
    if let Some(zone) = body.zone.as_deref().filter(|zone| !zone.is_empty()) {
        if let Err(e) = DateTimeService::set_timezone(zone).await {
            return PutDateTimeLegacyResponses::Ok(legacy_put_response(
                fail_response(format!("Could not set timezone: {}", e)),
                "",
            ));
        }
    } else if body.method.as_deref() == Some("manual") {
        if let Some(datetime) = body.datetime.as_deref().filter(|datetime| !datetime.is_empty()) {
            if let Ok(parsed) = datetime.parse::<i64>()
                && !CertificateProvisioningService::validate_new_timestamp(parsed, &tls_info).await
            {
                return PutDateTimeLegacyResponses::Ok(invalid_timestamp_legacy_response());
            }

            if let Err(e) = DateTimeService::set_time_manual(datetime).await {
                return PutDateTimeLegacyResponses::Ok(legacy_put_response(
                    fail_response(format!("Could not set datetime: {}", e)),
                    "",
                ));
            }

            let _ = ProvisioningStateMachine::handle(Event::ManualTimeSet).await;
        }
    } else {
        match DateTimeService::list_timezones().await {
            Ok(zones) => {
                let zone = DateTimeService::local_zone().await;
                let dt = DateTimeService::get_datetime(zones, zone);
                return PutDateTimeLegacyResponses::Ok(legacy_put_response(
                    ok_response(dt.zone),
                    dt.datetime,
                ));
            }
            Err(e) => {
                return PutDateTimeLegacyResponses::Ok(legacy_put_response(
                    fail_response(e.to_string()),
                    "",
                ));
            }
        }
    }

    match DateTimeService::list_timezones().await {
        Ok(zones) => {
            let zone = DateTimeService::local_zone().await;
            let dt = DateTimeService::get_datetime(zones, zone);
            PutDateTimeLegacyResponses::Ok(legacy_put_response(
                ok_response(dt.zone),
                dt.datetime,
            ))
        }
        Err(e) => PutDateTimeLegacyResponses::Ok(legacy_put_response(
            fail_response(e.to_string()),
            "",
        )),
    }
}
