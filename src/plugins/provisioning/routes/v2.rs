//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::provisioning::{CERT_TEMP_PATH, CONFIG_FILE_TEMP_PATH, DEVICE_SERVER_CSR_PATH};
use crate::plugins::provisioning::service::{
    CertificateProvisioningService, ProvisioningSaveError, ProvisioningState,
};
use axum::{extract::multipart::MultipartRejection, extract::Multipart};
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
    let state = CertificateProvisioningService::get_provisioning_state();
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
    let Ok(mut multipart) = multipart else {
        return ProvisioningTextResponses::BadRequest;
    };

    if CertificateProvisioningService::get_provisioning_state() != ProvisioningState::Unprovisioned {
        return ProvisioningTextResponses::BadRequest;
    }

    let mut config_file_found = false;
    let mut openssl_key_gen_args = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "configFile" => {
                let fname = field.file_name().unwrap_or("").to_string();
                if !fname.ends_with(".cnf") {
                    return ProvisioningTextResponses::BadRequest;
                }
                let data = field.bytes().await.unwrap_or_default();
                if std::fs::write(CONFIG_FILE_TEMP_PATH, data).is_err() {
                    return ProvisioningTextResponses::InternalError;
                }
                config_file_found = true;
            }
            "opensslKeyGenArgs" => {
                openssl_key_gen_args = field.text().await.unwrap_or_default();
            }
            _ => {}
        }
    }

    if !config_file_found {
        return ProvisioningTextResponses::BadRequest;
    }

    let key_gen_args = if openssl_key_gen_args.is_empty() { None } else { Some(openssl_key_gen_args.as_str()) };
    match CertificateProvisioningService::generate_key_and_csr(key_gen_args).await {
        Ok(_) => {
            let _ = std::fs::remove_file(CONFIG_FILE_TEMP_PATH);
            match tokio::fs::read(DEVICE_SERVER_CSR_PATH).await {
                Ok(data) => String::from_utf8_lossy(&data).into_owned().into(),
                Err(_) => ProvisioningTextResponses::InternalError,
            }
        }
        Err(e) => {
            error!("Couldn't generate key and CSR: {}", e);
            let _ = std::fs::remove_file(CONFIG_FILE_TEMP_PATH);
            ProvisioningTextResponses::InternalError
        }
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
    let Ok(mut multipart) = multipart else {
        return PutProvisioningResponses::BadRequest;
    };

    if CertificateProvisioningService::get_provisioning_state() != ProvisioningState::Unprovisioned {
        return PutProvisioningResponses::BadRequest;
    }

    let mut cert_file_found = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "certificate" {
            let fname = field.file_name().unwrap_or("").to_string();
            if !fname.ends_with(".crt") && !fname.ends_with(".pem") {
                return PutProvisioningResponses::BadRequest;
            }
            let data = field.bytes().await.unwrap_or_default();
            if std::fs::write(CERT_TEMP_PATH, data).is_err() {
                return PutProvisioningResponses::InternalError;
            }
            cert_file_found = true;
        }
    }

    if !cert_file_found {
        return PutProvisioningResponses::BadRequest;
    }

    match CertificateProvisioningService::save_certificate_file().await {
        Ok(_) => {
            tokio::spawn(async { CertificateProvisioningService::restart_summit_rcm().await });
            PutProvisioningResponses::Ok
        }
        Err(ProvisioningSaveError::InvalidCertificate) => {
            PutProvisioningResponses::BadRequest
        }
        Err(error) => {
            error!("Couldn't upload certificate file: {:?}", error);
            PutProvisioningResponses::InternalError
        }
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
    if CertificateProvisioningService::get_provisioning_state() != ProvisioningState::PartiallyProvisioned {
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
            if std::fs::write(TEMP_PATH, data).is_err() {
                return PutClientBundleResponses::InternalError;
            }
            cert_file_found = true;
        }
    }

    if !cert_file_found {
        let _ = std::fs::remove_file(TEMP_PATH);
        return PutClientBundleResponses::BadRequest;
    }

    match CertificateProvisioningService::save_paired_client_cert(TEMP_PATH).await {
        Ok(_) => {
            tokio::spawn(async { CertificateProvisioningService::restart_summit_rcm().await });
            PutClientBundleResponses::Ok
        }
        Err(ProvisioningSaveError::InvalidCertificate) => {
            let _ = std::fs::remove_file(TEMP_PATH);
            PutClientBundleResponses::BadRequest
        }
        Err(error) => {
            error!("Couldn't upload paired client certificate: {:?}", error);
            let _ = std::fs::remove_file(TEMP_PATH);
            PutClientBundleResponses::InternalError
        }
    }
}
