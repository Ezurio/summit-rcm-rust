//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::date_time::service::DateTimeService;
use crate::dbus;
use axum::Json;
use serde::{Deserialize, Serialize};
use log::error;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct DateTimeRequest {
    pub timezone: Option<String>,
    pub zone: Option<String>,
    pub datetime: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct DateTimeInfo {
    pub zones: Vec<String>,
    pub zone: String,
    pub datetime: String,
}

crate::define_json_response_family! {
    pub enum GetDateTimeResponses {
        Ok(DateTimeInfo) => 200;
        Timeout => 504,
        InternalError => 500
    }
    from DateTimeInfo => Ok;
}

pub type SetDateTimeResponses = GetDateTimeResponses;

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/datetime",
    tag = "system",
    responses(GetDateTimeResponses)
))]
pub async fn get_datetime() -> GetDateTimeResponses {
    match DateTimeService::get_datetime().await {
        Ok(v) => GetDateTimeResponses::Ok(DateTimeInfo {
            zones: v.zones,
            zone: v.zone,
            datetime: v.datetime,
        }),
        Err(e) => {
            error!("get_datetime: {}", e);
            if dbus::is_timeout_error(&e) {
                GetDateTimeResponses::Timeout
            } else {
                GetDateTimeResponses::InternalError
            }
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/datetime",
    tag = "system",
    request_body = DateTimeRequest,
    responses(SetDateTimeResponses)
))]
pub async fn set_datetime(Json(body): Json<DateTimeRequest>) -> SetDateTimeResponses {
    if let Some(tz) = body.zone.or(body.timezone) {
        if let Err(e) = DateTimeService::set_timezone(&tz).await {
            error!("set_datetime timezone: {}", e);
            return if dbus::is_timeout_error(&e) {
                SetDateTimeResponses::Timeout
            } else {
                SetDateTimeResponses::InternalError
            };
        }
    }

    if let Some(datetime) = body.datetime {
        if let Err(e) = DateTimeService::set_time_manual(&datetime).await {
            error!("set_datetime manual: {}", e);
            return if dbus::is_timeout_error(&e) {
                SetDateTimeResponses::Timeout
            } else {
                SetDateTimeResponses::InternalError
            };
        }
    }

    match DateTimeService::get_datetime().await {
        Ok(v) => SetDateTimeResponses::Ok(DateTimeInfo {
            zones: v.zones,
            zone: v.zone,
            datetime: v.datetime,
        }),
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
