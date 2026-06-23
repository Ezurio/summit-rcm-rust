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

#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyOperationResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
}

crate::define_ok_json_response_family! {
    pub enum LegacyOperationOkResponse(LegacyOperationResponse);
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