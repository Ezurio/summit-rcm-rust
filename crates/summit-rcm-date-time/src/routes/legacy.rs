//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm_web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::service::{DateTimeService, DateTimeSnapshot};
use summit_rcm_web::axum::Json;
use serde::{Deserialize, Serialize};

#[cfg(all(feature = "api-docs", feature = "api-legacy"))]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct DateTimeBody {
    pub zone: Option<String>,
    pub method: Option<String>,
    pub datetime: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyDateTimeResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zones: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub time: String,
}

summit_rcm_web::define_ok_json_response_family! {
    pub enum GetDateTimeLegacyResponses(LegacyDateTimeResponse);
}

pub type PutDateTimeLegacyResponses = GetDateTimeLegacyResponses;

fn legacy_datetime_response(
    operation: LegacyOperationResponse,
    zones: Option<Vec<String>>,
    zone: Option<String>,
    method: Option<String>,
    time: impl Into<String>,
) -> LegacyDateTimeResponse {
    LegacyDateTimeResponse {
        operation,
        zones,
        zone,
        method,
        time: time.into(),
    }
}

fn legacy_datetime_get_response(dt: DateTimeSnapshot) -> LegacyDateTimeResponse {
    legacy_datetime_response(
        ok_response(""),
        Some(dt.zones),
        Some(dt.zone),
        Some("auto".to_string()),
        dt.datetime,
    )
}

fn legacy_datetime_put_response(operation: LegacyOperationResponse, time: impl Into<String>) -> LegacyDateTimeResponse {
    legacy_datetime_response(operation, None, None, None, time)
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/datetime",
    tag = "legacy",
    responses(GetDateTimeLegacyResponses)
))]
pub async fn get_datetime_legacy() -> GetDateTimeLegacyResponses {
    match DateTimeService::list_timezones().await {
        Ok(zones) => {
            let zone = DateTimeService::local_zone().await;
            GetDateTimeLegacyResponses::Ok(legacy_datetime_get_response(DateTimeService::get_datetime(zones, zone)))
        }
        Err(e) => GetDateTimeLegacyResponses::Ok(legacy_datetime_response(
            fail_response(e.to_string()),
            Some(vec![]),
            Some("".to_string()),
            Some("auto".to_string()),
            "",
        )),
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/datetime",
    tag = "legacy",
    request_body = DateTimeBody,
    responses(PutDateTimeLegacyResponses)
))]
pub async fn put_datetime_legacy(Json(body): Json<DateTimeBody>) -> PutDateTimeLegacyResponses {
    if let Some(zone) = body.zone.as_deref().filter(|zone| !zone.is_empty()) {
        if let Err(e) = DateTimeService::set_timezone(zone).await {
            return PutDateTimeLegacyResponses::Ok(legacy_datetime_put_response(
                fail_response(format!("Could not set timezone: {}", e)),
                "",
            ));
        }
    } else if body.method.as_deref() == Some("manual") {
        if let Some(datetime) = body.datetime.as_deref().filter(|datetime| !datetime.is_empty())
            && let Err(e) = DateTimeService::set_time_manual(datetime).await {
                return PutDateTimeLegacyResponses::Ok(legacy_datetime_put_response(
                    fail_response(format!("Could not set datetime: {}", e)),
                    "",
                ));
            }
    } else {
        match DateTimeService::list_timezones().await {
            Ok(zones) => {
                let zone = DateTimeService::local_zone().await;
                let dt = DateTimeService::get_datetime(zones, zone);
                return PutDateTimeLegacyResponses::Ok(legacy_datetime_put_response(
                    ok_response(dt.zone),
                    dt.datetime,
                ));
            }
            Err(e) => {
                return PutDateTimeLegacyResponses::Ok(legacy_datetime_put_response(
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
            PutDateTimeLegacyResponses::Ok(legacy_datetime_put_response(
                ok_response(dt.zone),
                dt.datetime,
            ))
        }
        Err(e) => PutDateTimeLegacyResponses::Ok(legacy_datetime_put_response(
            fail_response(e.to_string()),
            "",
        )),
    }
}

