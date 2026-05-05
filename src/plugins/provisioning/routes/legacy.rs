//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::provisioning::service::{CertificateProvisioningService, ProvisioningState};
use axum::extract::multipart::MultipartRejection;
use axum::extract::Multipart;

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

crate::define_json_response_family! {
    pub enum GetProvisioningLegacyResponses {
        Ok(LegacyProvisioningStateResponse) => 200;
    }
    from LegacyProvisioningStateResponse => Ok;
}

crate::define_json_response_family! {
    pub enum PutProvisioningLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyProvisioningStateResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    pub state: i32,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/certificateProvisioning",
    tag = "provisioning",
    responses(GetProvisioningLegacyResponses)
))]
pub async fn get_provisioning_legacy() -> GetProvisioningLegacyResponses {
    let state = CertificateProvisioningService::get_provisioning_state();
    LegacyProvisioningStateResponse {
        sdcerr: 0,
        info_msg: "".to_string(),
        state: state as i32,
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/certificateProvisioning",
    tag = "provisioning",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(super::v2::ProvisioningTextResponses)
))]
pub async fn post_provisioning_legacy(
    multipart: Result<Multipart, MultipartRejection>,
) -> super::v2::ProvisioningTextResponses {
    if CertificateProvisioningService::get_provisioning_state() != ProvisioningState::Unprovisioned {
        return super::v2::ProvisioningTextResponses::BadRequest;
    }

    let Ok(multipart) = multipart else {
        return super::v2::ProvisioningTextResponses::BadRequest;
    };

    match super::v2::post_provisioning(Ok(multipart)).await {
        super::v2::ProvisioningTextResponses::Ok(body) => super::v2::ProvisioningTextResponses::Ok(body),
        super::v2::ProvisioningTextResponses::BadRequest => super::v2::ProvisioningTextResponses::BadRequest,
        super::v2::ProvisioningTextResponses::InternalError => super::v2::ProvisioningTextResponses::InternalError,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/certificateProvisioning",
    tag = "provisioning",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(PutProvisioningLegacyResponses)
))]
pub async fn put_provisioning_legacy(
    multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
) -> PutProvisioningLegacyResponses {
    if CertificateProvisioningService::get_provisioning_state() != ProvisioningState::Unprovisioned {
        return fail_response("Already provisioned").into();
    }

    let Ok(multipart) = multipart else {
        return fail_response("Could not upload certificate file").into();
    };

    match super::v2::put_provisioning(Ok(multipart)).await {
        super::v2::PutProvisioningResponses::Ok => ok_response("").into(),
        super::v2::PutProvisioningResponses::BadRequest | super::v2::PutProvisioningResponses::InternalError => {
            fail_response("Could not upload certificate file").into()
        }
    }
}
