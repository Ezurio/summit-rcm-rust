//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, LegacyOperationResponse};

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct TimestampValidityPayload {
    pub time: String,
    #[serde(rename = "notBefore", skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
}

#[cfg(feature = "api-legacy")]
#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyTimestampValidationResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(flatten)]
    pub validity: TimestampValidityPayload,
}

#[cfg(feature = "api-legacy")]
crate::define_json_response_family! {
    pub enum InvalidTimestampResponses {
        Legacy(LegacyTimestampValidationResponse) => 200,
        V2(TimestampValidityPayload) => 400;
    }
    from TimestampValidityPayload => V2;
}

#[cfg(not(feature = "api-legacy"))]
crate::define_json_response_family! {
    pub enum InvalidTimestampResponses {
        V2(TimestampValidityPayload) => 400;
    }
    from TimestampValidityPayload => V2;
}

#[cfg(feature = "api-legacy")]
fn legacy_invalid_timestamp_response(
    validity: TimestampValidityPayload,
) -> LegacyTimestampValidationResponse {
    LegacyTimestampValidationResponse {
        operation: fail_response("Invalid timestamp"),
        validity,
    }
}

pub fn invalid_timestamp_response(path: &str, validity: TimestampValidityPayload) -> InvalidTimestampResponses {
    #[cfg(feature = "api-legacy")]
    if path == "/datetime" {
        return InvalidTimestampResponses::Legacy(legacy_invalid_timestamp_response(validity));
    }

    validity.into()
}
