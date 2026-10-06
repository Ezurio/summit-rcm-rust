//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::DateTimeService;
use log::error;
use serde::{Deserialize, Serialize};
use summit_rcm_core::dbus;
use summit_rcm_web::axum::Json;

#[cfg(all(feature = "api-docs", feature = "api-v2"))]
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

fn date_time_info(zones: Vec<String>, zone: String, datetime: String) -> DateTimeInfo {
    DateTimeInfo {
        zones,
        zone,
        datetime,
    }
}

summit_rcm_web::define_json_response_family! {
    pub enum GetDateTimeResponses {
        Ok(DateTimeInfo) => 200;
        Timeout => 504,
        InternalError => 500
    }
    from DateTimeInfo => Ok;
}

summit_rcm_web::define_json_response_family! {
    pub enum SetDateTimeResponses {
        Ok(DateTimeInfo) => 200,
        BadRequest(DateTimeInfo) => 400;
        Timeout => 504,
        InternalError => 500
    }
    from DateTimeInfo => Ok;
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/api/v2/system/datetime",
        tag = "system",
        responses(GetDateTimeResponses)
    )
)]
pub async fn get_datetime() -> GetDateTimeResponses {
    match DateTimeService::list_timezones().await {
        Ok(zones) => {
            let zone = DateTimeService::local_zone().await;
            let v = DateTimeService::get_datetime(zones, zone);
            GetDateTimeResponses::Ok(date_time_info(v.zones, v.zone, v.datetime))
        }
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
    let zone = body.zone.or(body.timezone);
    if let Some(tz) = zone.clone()
        && let Err(e) = DateTimeService::set_timezone(&tz).await
    {
        error!("set_datetime timezone: {}", e);
        return if dbus::is_timeout_error(&e) {
            SetDateTimeResponses::Timeout
        } else {
            SetDateTimeResponses::InternalError
        };
    }

    if let Some(datetime) = body.datetime
        && let Err(e) = DateTimeService::set_time_manual(&datetime).await
    {
        error!("set_datetime manual: {}", e);
        return if dbus::is_timeout_error(&e) {
            SetDateTimeResponses::Timeout
        } else {
            SetDateTimeResponses::InternalError
        };
    }

    match DateTimeService::list_timezones().await {
        Ok(zones) => {
            let zone = DateTimeService::local_zone().await;
            let v = DateTimeService::get_datetime(zones, zone);
            SetDateTimeResponses::Ok(date_time_info(v.zones, v.zone, v.datetime))
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
