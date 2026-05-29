//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::plugins::provisioning::routes::shared::{
    create_csr_from_upload, save_uploaded_certificate, ProvisioningRouteError,
};
use crate::plugins::provisioning::service::CertificateProvisioningService;
use axum::{
    extract::multipart::MultipartRejection,
    extract::Multipart,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

crate::define_ok_json_response_family! {
    pub enum GetProvisioningLegacyResponses(LegacyProvisioningStateResponse);
}

pub type PostProvisioningLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;
pub type PutProvisioningLegacyResponses = crate::web::legacy_response::LegacyOperationOkResponse;

#[derive(serde::Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyProvisioningStateResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    pub state: i32,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/certificateProvisioning",
    tag = "provisioning",
    responses(GetProvisioningLegacyResponses)
))]
pub async fn get_provisioning_legacy() -> GetProvisioningLegacyResponses {
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
pub async fn post_provisioning_legacy(
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    match create_csr_from_upload(multipart).await {
        Ok(csr) => ([(header::CONTENT_TYPE, "application/x-download")], csr).into_response(),
        Err(ProvisioningRouteError::BadRequest)
        | Err(ProvisioningRouteError::AlreadyProvisioned)
        | Err(ProvisioningRouteError::MissingFilename)
        | Err(ProvisioningRouteError::InvalidCertificate) => StatusCode::BAD_REQUEST.into_response(),
        Err(ProvisioningRouteError::InternalError) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
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
    match save_uploaded_certificate(multipart).await {
        Ok(()) => ok_response("").into(),
        Err(ProvisioningRouteError::AlreadyProvisioned) => fail_response("Already provisioned").into(),
        Err(ProvisioningRouteError::MissingFilename) => fail_response("No filename specified").into(),
        Err(ProvisioningRouteError::BadRequest) | Err(ProvisioningRouteError::InvalidCertificate) => {
            fail_response("Invalid certificate file").into()
        }
        Err(ProvisioningRouteError::InternalError) => fail_response("Error uploading certificate file").into(),
    }
}
