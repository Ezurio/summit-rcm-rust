//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Date/time management service (D-Bus timedate1 + timedatectl)

use crate::dbus;
use crate::definition::SUMMIT_RCM_TIME_FORMAT_DESCRIPTION;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use time::PrimitiveDateTime;

const TIMEDATE1_BUS_NAME: &str = "org.freedesktop.timedate1";
const TIMEDATE1_MAIN_OBJ: &str = "/org/freedesktop/timedate1";

pub struct DateTimeService;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct DateTimeSnapshot {
    pub zones: Vec<String>,
    pub zone: String,
    pub datetime: String,
}

impl DateTimeService {
    fn local_now() -> Result<OffsetDateTime> {
        OffsetDateTime::now_local().map_err(Into::into)
    }

    pub async fn local_zone() -> String {
        tokio::fs::read_to_string("/etc/timezone")
            .await
            .map(|zone| zone.trim().to_string())
            .unwrap_or_else(|_| "Unable to determine timezone".to_string())
    }

    pub fn check_current_date_and_time() -> (bool, String) {
        match Self::local_now() {
            Ok(now) => (
                true,
                now.format(SUMMIT_RCM_TIME_FORMAT_DESCRIPTION)
                    .unwrap_or_else(|error| error.to_string()),
            ),
            Err(error) => (false, error.to_string()),
        }
    }

    pub async fn get_datetime() -> Result<DateTimeSnapshot> {
        let zones = Self::list_timezones().await?;
        let (success, current_datetime) = Self::check_current_date_and_time();
        let datetime = if success { current_datetime } else { String::new() };

        Ok(DateTimeSnapshot {
            zones,
            zone: Self::local_zone().await,
            datetime,
        })
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
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
        dbus::call_method(
            conn,
            Some(TIMEDATE1_BUS_NAME),
            TIMEDATE1_MAIN_OBJ,
            Some("org.freedesktop.timedate1"),
            "SetTime",
            &(dt_int, false, false),
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn set_timezone(timezone: &str) -> Result<()> {
        let conn = dbus::system_bus().await?;
        dbus::call_method(
            conn,
            Some(TIMEDATE1_BUS_NAME),
            TIMEDATE1_MAIN_OBJ,
            Some("org.freedesktop.timedate1"),
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
