//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::routes::shared::{
    ProvisioningRouteError, create_csr_from_upload, save_uploaded_certificate,
};
use crate::service::{CertificateProvisioningService, ClientTlsInfo};
use crate::state_machine::{Event, ProvisioningStateMachine};
use summit_rcm_date_time::routes::legacy::DateTimeBody;
use summit_rcm_date_time::service::DateTimeService;
use summit_rcm_web::axum::{
    Json, extract::Extension, extract::Multipart, extract::multipart::MultipartRejection,
};
use summit_rcm_web::legacy_response::{LegacyOperationResponse, fail_response, ok_response};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

pub(crate) use summit_rcm_date_time::routes::legacy::get_datetime_legacy;

summit_rcm_web::define_ok_json_response_family! {
    pub(crate) enum GetProvisioningLegacyResponses(LegacyProvisioningStateResponse);
}

summit_rcm_web::define_text_response_family! {
    pub(crate) enum PostProvisioningLegacyResponses {
        Ok(String) => "application/x-download";
        BadRequest => 400,
        InternalError => 500
    }
    from String => Ok;
}

pub(crate) type PutProvisioningLegacyResponses =
    summit_rcm_web::legacy_response::LegacyOperationOkResponse;

summit_rcm_web::define_ok_json_response_family! {
    pub(crate) enum PutProvisioningDateTimeLegacyResponses(ProvisioningLegacyDateTimeResponse);
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct LegacyProvisioningStateResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub state: i32,
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct ProvisioningLegacyDateTimeResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zones: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub time: String,
    #[serde(rename = "notBefore", skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
}

async fn invalid_timestamp_legacy_response(
    tls_info: &ClientTlsInfo,
) -> ProvisioningLegacyDateTimeResponse {
    let validity = CertificateProvisioningService::timestamp_validity_payload(tls_info).await;
    ProvisioningLegacyDateTimeResponse {
        operation: fail_response("Invalid timestamp"),
        zones: None,
        zone: None,
        method: None,
        time: validity.time,
        not_before: validity.not_before,
        not_after: validity.not_after,
    }
}

fn legacy_put_response(
    operation: LegacyOperationResponse,
    time: impl Into<String>,
) -> ProvisioningLegacyDateTimeResponse {
    ProvisioningLegacyDateTimeResponse {
        operation,
        zones: None,
        zone: None,
        method: None,
        time: time.into(),
        not_before: None,
        not_after: None,
    }
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/certificateProvisioning",
        tag = "provisioning",
        responses(GetProvisioningLegacyResponses)
    )
)]
pub(crate) async fn get_provisioning_legacy() -> GetProvisioningLegacyResponses {
    let state = CertificateProvisioningService::get_provisioning_state_async().await;
    LegacyProvisioningStateResponse {
        operation: ok_response(""),
        state: state as i32,
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/certificateProvisioning",
    tag = "provisioning",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(PostProvisioningLegacyResponses)
))]
pub(crate) async fn post_provisioning_legacy(
    multipart: Result<Multipart, MultipartRejection>,
) -> PostProvisioningLegacyResponses {
    match create_csr_from_upload(multipart).await {
        Ok(csr) => csr.into(),
        Err(ProvisioningRouteError::BadRequest)
        | Err(ProvisioningRouteError::AlreadyProvisioned)
        | Err(ProvisioningRouteError::MissingFilename)
        | Err(ProvisioningRouteError::InvalidCertificate) => {
            PostProvisioningLegacyResponses::BadRequest
        }
        Err(ProvisioningRouteError::InternalError) => {
            PostProvisioningLegacyResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/certificateProvisioning",
    tag = "provisioning",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(PutProvisioningLegacyResponses)
))]
pub(crate) async fn put_provisioning_legacy(
    multipart: Result<Multipart, MultipartRejection>,
) -> PutProvisioningLegacyResponses {
    match save_uploaded_certificate(multipart).await {
        Ok(()) => ok_response("").into(),
        Err(ProvisioningRouteError::AlreadyProvisioned) => {
            fail_response("Already provisioned").into()
        }
        Err(ProvisioningRouteError::MissingFilename) => {
            fail_response("No filename specified").into()
        }
        Err(ProvisioningRouteError::BadRequest)
        | Err(ProvisioningRouteError::InvalidCertificate) => {
            fail_response("Invalid certificate file").into()
        }
        Err(ProvisioningRouteError::InternalError) => {
            fail_response("Error uploading certificate file").into()
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/datetime",
    tag = "legacy",
    request_body = DateTimeBody,
    responses(PutProvisioningDateTimeLegacyResponses)
))]
pub(crate) async fn put_datetime_legacy(
    Extension(tls_info): Extension<ClientTlsInfo>,
    Json(body): Json<DateTimeBody>,
) -> PutProvisioningDateTimeLegacyResponses {
    if let Some(zone) = body.zone.as_deref().filter(|zone| !zone.is_empty()) {
        if let Err(e) = DateTimeService::set_timezone(zone).await {
            return PutProvisioningDateTimeLegacyResponses::Ok(legacy_put_response(
                fail_response(format!("Could not set timezone: {}", e)),
                "",
            ));
        }
    } else if body.method.as_deref() == Some("manual") {
        if let Some(datetime) = body
            .datetime
            .as_deref()
            .filter(|datetime| !datetime.is_empty())
        {
            if let Ok(parsed) = datetime.parse::<i64>()
                && !CertificateProvisioningService::validate_new_timestamp(parsed, &tls_info).await
            {
                return PutProvisioningDateTimeLegacyResponses::Ok(
                    invalid_timestamp_legacy_response(&tls_info).await,
                );
            }

            if let Err(e) = DateTimeService::set_time_manual(datetime).await {
                return PutProvisioningDateTimeLegacyResponses::Ok(legacy_put_response(
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
                return PutProvisioningDateTimeLegacyResponses::Ok(legacy_put_response(
                    ok_response(dt.zone),
                    dt.datetime,
                ));
            }
            Err(e) => {
                return PutProvisioningDateTimeLegacyResponses::Ok(legacy_put_response(
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
            PutProvisioningDateTimeLegacyResponses::Ok(legacy_put_response(
                ok_response(dt.zone),
                dt.datetime,
            ))
        }
        Err(e) => PutProvisioningDateTimeLegacyResponses::Ok(legacy_put_response(
            fail_response(e.to_string()),
            "",
        )),
    }
}
