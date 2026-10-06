//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Date/time management service (D-Bus timedate1 + timedatectl)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use summit_rcm_core::dbus;
use summit_rcm_core::definition::SUMMIT_RCM_TIME_FORMAT_DESCRIPTION;
use summit_rcm_core::utils::read_text;
use time::{OffsetDateTime, PrimitiveDateTime, util};

const TIMEDATE1_BUS_NAME: &str = "org.freedesktop.timedate1";
const TIMEDATE1_MAIN_OBJ: &str = "/org/freedesktop/timedate1";
const TIMEDATE1_MAIN_IFACE: &str = "org.freedesktop.timedate1";

pub struct DateTimeService;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct DateTimeSnapshot {
    pub zones: Vec<String>,
    pub zone: String,
    pub datetime: String,
}

impl DateTimeService {
    pub async fn local_zone() -> String {
        read_text("/etc/timezone")
            .await
            .map(|zone| zone.trim().to_string())
            .unwrap_or_else(|_| "Unable to determine timezone".to_string())
    }

    pub fn current_datetime() -> String {
        let _ = util::refresh_tz();
        OffsetDateTime::now_local()
            .unwrap_or_else(|_| OffsetDateTime::now_utc())
            .format(SUMMIT_RCM_TIME_FORMAT_DESCRIPTION)
            .expect("static datetime format should always format OffsetDateTime")
    }

    /// Current time as a UTC string in the standard summit-rcm format.
    pub fn now_utc_formatted() -> String {
        OffsetDateTime::now_utc()
            .format(SUMMIT_RCM_TIME_FORMAT_DESCRIPTION)
            .unwrap_or_else(|t| t.to_string())
    }

    pub fn get_datetime(zones: Vec<String>, zone: String) -> DateTimeSnapshot {
        DateTimeSnapshot {
            zones,
            zone,
            datetime: Self::current_datetime(),
        }
    }

    pub async fn set_time_manual(dt: &str) -> Result<()> {
        let dt_int = match dt.parse::<i64>() {
            Ok(value) => value,
            Err(_) => {
                let parsed = PrimitiveDateTime::parse(dt, SUMMIT_RCM_TIME_FORMAT_DESCRIPTION)
                    .map_err(|_| anyhow::anyhow!("Unable to parse datetime"))?;
                (parsed.as_utc().unix_timestamp_nanos() / 1_000)
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Parsed datetime is out of range"))?
            }
        };

        let conn = dbus::system_bus().await?;
        let _ = dbus::call_method(
            conn,
            Some(TIMEDATE1_BUS_NAME),
            TIMEDATE1_MAIN_OBJ,
            Some(TIMEDATE1_MAIN_IFACE),
            "SetTime",
            &(dt_int, false, false),
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn set_timezone(timezone: &str) -> Result<()> {
        let conn = dbus::system_bus().await?;
        let _ = dbus::call_method(
            conn,
            Some(TIMEDATE1_BUS_NAME),
            TIMEDATE1_MAIN_OBJ,
            Some(TIMEDATE1_MAIN_IFACE),
            "SetTimezone",
            &(timezone, false),
            None,
        )
        .await?;

        Ok(())
    }

    pub async fn list_timezones() -> Result<Vec<String>> {
        let conn = dbus::system_bus().await?;
        dbus::call_method_deserialize_with_timeout(
            conn,
            Some(TIMEDATE1_BUS_NAME),
            TIMEDATE1_MAIN_OBJ,
            Some("org.freedesktop.timedate1"),
            "ListTimezones",
            &(),
            None,
        )
        .await
    }
}
