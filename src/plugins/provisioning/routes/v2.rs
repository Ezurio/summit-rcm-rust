//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::provisioning::routes::shared::{
    create_csr_from_upload, save_uploaded_certificate, ProvisioningRouteError,
};
use crate::plugins::provisioning::service::{
    CertificateProvisioningService, ProvisioningSaveError, ProvisioningState,
};
use crate::plugins::provisioning::state_machine::{Event, ProvisioningStateMachine};
use axum::{
    extract::multipart::MultipartRejection,
    extract::Multipart,
};
use log::error;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::openapi_doc;

crate::define_ok_json_response_family! {
    pub enum GetProvisioningResponses(ProvisioningStateResponse);
}

crate::define_status_response_family! {
    pub enum PutProvisioningResponses {
        Ok => 200,
        BadRequest => 400,
        InternalError => 500
    }
}

pub type PutClientBundleResponses = PutProvisioningResponses;

crate::define_text_response_family! {
    pub enum ProvisioningTextResponses {
        Ok(String) => "text/plain";
        BadRequest => 400,
        InternalError => 500
    }
    from String => Ok;
}

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(serde::Serialize)]
pub struct ProvisioningStateResponse {
    pub state: i32,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/system/certificateProvisioning",
    tag = "provisioning",
    responses(GetProvisioningResponses)
))]
pub async fn get_provisioning() -> GetProvisioningResponses {
    let state = CertificateProvisioningService::get_provisioning_state_async().await;
    ProvisioningStateResponse { state: state as i32 }.into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/system/certificateProvisioning",
    tag = "provisioning",
    request_body(content = String, content_type = "multipart/form-data",
        description = "configFile (.cnf) and optional opensslKeyGenArgs"),
    responses(
        ProvisioningTextResponses,
    )
))]
pub async fn post_provisioning(
    multipart: Result<Multipart, MultipartRejection>,
) -> ProvisioningTextResponses {
    match create_csr_from_upload(multipart).await {
        Ok(csr) => csr.into(),
        Err(ProvisioningRouteError::BadRequest)
        | Err(ProvisioningRouteError::AlreadyProvisioned)
        | Err(ProvisioningRouteError::MissingFilename)
        | Err(ProvisioningRouteError::InvalidCertificate) => ProvisioningTextResponses::BadRequest,
        Err(ProvisioningRouteError::InternalError) => ProvisioningTextResponses::InternalError,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/certificateProvisioning",
    tag = "provisioning",
    request_body(content = String, content_type = "multipart/form-data",
        description = "certificate (.crt or .pem)"),
    responses(
        PutProvisioningResponses,
    )
))]
pub async fn put_provisioning(
    multipart: Result<Multipart, MultipartRejection>,
) -> PutProvisioningResponses {
    match save_uploaded_certificate(multipart).await {
        Ok(()) => PutProvisioningResponses::Ok,
        Err(ProvisioningRouteError::BadRequest)
        | Err(ProvisioningRouteError::AlreadyProvisioned)
        | Err(ProvisioningRouteError::MissingFilename)
        | Err(ProvisioningRouteError::InvalidCertificate) => PutProvisioningResponses::BadRequest,
        Err(ProvisioningRouteError::InternalError) => PutProvisioningResponses::InternalError,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/certificateProvisioning/clientBundle",
    tag = "provisioning",
    request_body(content = String, content_type = "multipart/form-data",
        description = "certificate (.crt or .pem) for client pairing"),
    responses(
        PutClientBundleResponses,
    )
))]
pub async fn put_client_bundle(mut multipart: Multipart) -> PutClientBundleResponses {
    if CertificateProvisioningService::get_provisioning_state_async().await
        != ProvisioningState::PartiallyProvisioned
    {
        return PutClientBundleResponses::BadRequest;
    }

    const TEMP_PATH: &str = "/tmp/paired_client.crt";
    let mut cert_file_found = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "certificate" {
            let fname = field.file_name().unwrap_or("").to_string();
            if !fname.ends_with(".crt") && !fname.ends_with(".pem") {
                return PutClientBundleResponses::BadRequest;
            }
            let data = field.bytes().await.unwrap_or_default();
            if tokio::fs::write(TEMP_PATH, data).await.is_err() {
                return PutClientBundleResponses::InternalError;
            }
            cert_file_found = true;
        }
    }

    if !cert_file_found {
        let _ = tokio::fs::remove_file(TEMP_PATH).await;
        return PutClientBundleResponses::BadRequest;
    }

    match CertificateProvisioningService::save_paired_client_cert(TEMP_PATH).await {
        Ok(_) => match ProvisioningStateMachine::handle(Event::ClientBundleUploaded).await {
            Ok(_) => PutClientBundleResponses::Ok,
            Err(error) => {
                error!("Couldn't process client bundle upload: {:?}", error);
                PutClientBundleResponses::InternalError
            }
        },
        Err(ProvisioningSaveError::InvalidCertificate) => {
            let _ = tokio::fs::remove_file(TEMP_PATH).await;
            PutClientBundleResponses::BadRequest
        }
        Err(error) => {
            error!("Couldn't upload paired client certificate: {:?}", error);
            let _ = tokio::fs::remove_file(TEMP_PATH).await;
            PutClientBundleResponses::InternalError
        }
    }
}
