//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::paths::{CERT_TEMP_PATH, CONFIG_FILE_TEMP_PATH, DEVICE_SERVER_CSR_PATH};
use crate::service::{CertificateProvisioningService, ProvisioningSaveError, ProvisioningState};
use crate::state_machine::{Event, ProvisioningStateMachine};
use log::error;
use summit_rcm_web::axum::{extract::Multipart, extract::multipart::MultipartRejection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProvisioningRouteError {
    BadRequest,
    AlreadyProvisioned,
    MissingFilename,
    InvalidCertificate,
    InternalError,
}

pub(crate) async fn create_csr_from_upload(
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<String, ProvisioningRouteError> {
    if CertificateProvisioningService::get_provisioning_state_async().await
        != ProvisioningState::Unprovisioned
    {
        return Err(ProvisioningRouteError::AlreadyProvisioned);
    }

    let Ok(mut multipart) = multipart else {
        return Err(ProvisioningRouteError::BadRequest);
    };

    let mut config_file_found = false;
    let mut openssl_key_gen_args = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "configFile" => {
                let fname = field.file_name().unwrap_or("").to_string();
                if !fname.ends_with(".cnf") {
                    return Err(ProvisioningRouteError::BadRequest);
                }
                let data = field
                    .bytes()
                    .await
                    .map_err(|_| ProvisioningRouteError::BadRequest)?;
                if tokio::fs::write(CONFIG_FILE_TEMP_PATH, data).await.is_err() {
                    return Err(ProvisioningRouteError::InternalError);
                }
                config_file_found = true;
            }
            "opensslKeyGenArgs" => {
                // Optional field; empty string means "use defaults", so a read
                // error is treated the same as "not provided".
                openssl_key_gen_args = field.text().await.unwrap_or_default();
            }
            _ => {}
        }
    }

    if !config_file_found {
        return Err(ProvisioningRouteError::BadRequest);
    }

    let key_gen_args = if openssl_key_gen_args.is_empty() {
        None
    } else {
        Some(openssl_key_gen_args.as_str())
    };

    match CertificateProvisioningService::generate_key_and_csr(key_gen_args).await {
        Ok(_) => {
            let _ = tokio::fs::remove_file(CONFIG_FILE_TEMP_PATH).await;
            match tokio::fs::read(DEVICE_SERVER_CSR_PATH).await {
                Ok(data) => Ok(String::from_utf8_lossy(&data).into_owned()),
                Err(_) => Err(ProvisioningRouteError::InternalError),
            }
        }
        Err(e) => {
            error!("Couldn't generate key and CSR: {}", e);
            let _ = tokio::fs::remove_file(CONFIG_FILE_TEMP_PATH).await;
            Err(ProvisioningRouteError::InternalError)
        }
    }
}

pub(crate) async fn save_uploaded_certificate(
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(), ProvisioningRouteError> {
    if CertificateProvisioningService::get_provisioning_state_async().await
        != ProvisioningState::Unprovisioned
    {
        return Err(ProvisioningRouteError::AlreadyProvisioned);
    }

    let Ok(mut multipart) = multipart else {
        return Err(ProvisioningRouteError::InvalidCertificate);
    };

    let mut cert_file_found = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "certificate" {
            let fname = field.file_name().unwrap_or("").to_string();
            if !fname.ends_with(".crt") && !fname.ends_with(".pem") {
                return Err(ProvisioningRouteError::InvalidCertificate);
            }
            let data = field
                .bytes()
                .await
                .map_err(|_| ProvisioningRouteError::BadRequest)?;
            if tokio::fs::write(CERT_TEMP_PATH, data).await.is_err() {
                return Err(ProvisioningRouteError::InternalError);
            }
            cert_file_found = true;
        }
    }

    if !cert_file_found {
        return Err(ProvisioningRouteError::MissingFilename);
    }

    match CertificateProvisioningService::save_certificate_file().await {
        Ok(_) => match ProvisioningStateMachine::handle(Event::CertUploaded).await {
            Ok(_) => Ok(()),
            Err(error) => {
                error!(
                    "Couldn't transition provisioning state after cert upload: {:?}",
                    error
                );
                Err(ProvisioningRouteError::InternalError)
            }
        },
        Err(ProvisioningSaveError::InvalidCertificate) => {
            Err(ProvisioningRouteError::InvalidCertificate)
        }
        Err(error) => {
            error!("Couldn't upload certificate file: {:?}", error);
            Err(ProvisioningRouteError::InternalError)
        }
    }
}
