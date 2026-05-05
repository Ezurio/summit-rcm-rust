//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdcerrCode {
    Success = 0,
    Fail = 1,
    UserLogged = 2,
    UserBlocked = 3,
    SessionCheckFailed = 4,
    FirmwareUpdating = 5,
}

impl SdcerrCode {
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyOperationResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacySdcerrDefinitions {
    #[serde(rename = "SDCERR_SUCCESS")]
    pub success: i32,
    #[serde(rename = "SDCERR_FAIL")]
    pub fail: i32,
    #[serde(rename = "SDCERR_USER_LOGGED")]
    pub user_logged: i32,
    #[serde(rename = "SDCERR_USER_BLOCKED")]
    pub user_blocked: i32,
    #[serde(rename = "SDCERR_SESSION_CHECK_FAILED")]
    pub session_check_failed: i32,
    #[serde(rename = "SDCERR_FIRMWARE_UPDATING")]
    pub firmware_updating: i32,
}

#[cfg(any(feature = "stunnel", feature = "log-forwarding"))]
#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyStateResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

pub fn ok_response(info_msg: impl Into<String>) -> LegacyOperationResponse {
    LegacyOperationResponse {
        sdcerr: SdcerrCode::Success.as_i32(),
        info_msg: info_msg.into(),
    }
}

pub fn fail_response(info_msg: impl Into<String>) -> LegacyOperationResponse {
    LegacyOperationResponse {
        sdcerr: SdcerrCode::Fail.as_i32(),
        info_msg: info_msg.into(),
    }
}

#[cfg(any(feature = "stunnel", feature = "log-forwarding"))]
pub fn legacy_state_model(state: String, error_msg: &str) -> LegacyStateResponse {
    LegacyStateResponse {
        sdcerr: if state != "unknown" {
            SdcerrCode::Success.as_i32()
        } else {
            SdcerrCode::Fail.as_i32()
        },
        info_msg: if state != "unknown" {
            "".to_string()
        } else {
            error_msg.to_string()
        },
        state: Some(state),
    }
}

pub fn summit_rcm_errors() -> LegacySdcerrDefinitions {
    LegacySdcerrDefinitions {
        success: SdcerrCode::Success.as_i32(),
        fail: SdcerrCode::Fail.as_i32(),
        user_logged: SdcerrCode::UserLogged.as_i32(),
        user_blocked: SdcerrCode::UserBlocked.as_i32(),
        session_check_failed: SdcerrCode::SessionCheckFailed.as_i32(),
        firmware_updating: SdcerrCode::FirmwareUpdating.as_i32(),
    }
}