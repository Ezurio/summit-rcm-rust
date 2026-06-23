//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::routes::shared::{
    create_csr_from_upload, save_uploaded_certificate, ProvisioningRouteError,
};
use crate::service::{
    CertificateProvisioningService, ClientTlsInfo, ProvisioningSaveError, ProvisioningState,
};
use crate::state_machine::{Event, ProvisioningStateMachine};
use summit_rcm_web::axum::{
    extract::multipart::MultipartRejection,
    extract::Multipart,
    extract::Extension,
    Json,
};
use log::error;
use summit_rcm_core::dbus;
use summit_rcm_date_time::routes::v2::DateTimeRequest;
use summit_rcm_date_time::service::{DateTimeService, DateTimeSnapshot};

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::openapi_doc;

pub use summit_rcm_date_time::routes::v2::get_datetime;

summit_rcm_web::define_ok_json_response_family! {
    pub enum GetProvisioningResponses(ProvisioningStateResponse);
}

summit_rcm_web::define_status_response_family! {
    pub enum PutProvisioningResponses {
        Ok => 200,
        BadRequest => 400,
        InternalError => 500
    }
}

pub type PutClientBundleResponses = PutProvisioningResponses;

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct ProvisioningDateTimeInfo {
    pub zones: Vec<String>,
    pub zone: String,
    pub datetime: String,
    #[serde(rename = "notBefore", skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
}

summit_rcm_web::define_json_response_family! {
    pub enum ProvisioningSetDateTimeResponses {
        Ok(ProvisioningDateTimeInfo) => 200,
        BadRequest(ProvisioningDateTimeInfo) => 400;
        Timeout => 504,
        InternalError => 500
    }
    from ProvisioningDateTimeInfo => Ok;
}

summit_rcm_web::define_text_response_family! {
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

fn provisioning_datetime_info(info: DateTimeSnapshot) -> ProvisioningDateTimeInfo {
    ProvisioningDateTimeInfo {
        zones: info.zones,
        zone: info.zone,
        datetime: info.datetime,
        not_before: None,
        not_after: None,
    }
}

async fn invalid_timestamp_info(tls_info: &ClientTlsInfo) -> ProvisioningDateTimeInfo {
    let validity = CertificateProvisioningService::timestamp_validity_payload(tls_info).await;
    ProvisioningDateTimeInfo {
        zones: vec![],
        zone: String::new(),
        datetime: validity.time,
        not_before: validity.not_before,
        not_after: validity.not_after,
    }
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
    path = "/api/v2/system/datetime",
    tag = "system",
    request_body = DateTimeRequest,
    responses(ProvisioningSetDateTimeResponses)
))]
pub async fn put_datetime(
    Extension(tls_info): Extension<ClientTlsInfo>,
    Json(body): Json<DateTimeRequest>,
) -> ProvisioningSetDateTimeResponses {
    let zone = body.zone.or(body.timezone);
    if let Some(tz) = zone.clone()
        && let Err(e) = DateTimeService::set_timezone(&tz).await {
            error!("set_datetime timezone: {}", e);
            return if dbus::is_timeout_error(&e) {
                ProvisioningSetDateTimeResponses::Timeout
            } else {
                ProvisioningSetDateTimeResponses::InternalError
            };
        }

    let mut manual_time_set_request = false;
    if let Some(datetime) = body.datetime {
        if zone.as_deref().unwrap_or_default().is_empty() {
            manual_time_set_request = true;
            if let Ok(parsed) = datetime.parse::<i64>()
                && !CertificateProvisioningService::validate_new_timestamp(parsed, &tls_info).await
            {
                return ProvisioningSetDateTimeResponses::BadRequest(invalid_timestamp_info(&tls_info).await);
            }
        }

        if let Err(e) = DateTimeService::set_time_manual(&datetime).await {
            error!("set_datetime manual: {}", e);
            return if dbus::is_timeout_error(&e) {
                ProvisioningSetDateTimeResponses::Timeout
            } else {
                ProvisioningSetDateTimeResponses::InternalError
            };
        }
    }

    if manual_time_set_request {
        let _ = ProvisioningStateMachine::handle(Event::ManualTimeSet).await;
    }

    match DateTimeService::list_timezones().await {
        Ok(zones) => {
            let zone = DateTimeService::local_zone().await;
            let v = DateTimeService::get_datetime(zones, zone);
            ProvisioningSetDateTimeResponses::Ok(provisioning_datetime_info(v))
        }
        Err(e) => {
            error!("set_datetime get: {}", e);
            if dbus::is_timeout_error(&e) {
                ProvisioningSetDateTimeResponses::Timeout
            } else {
                ProvisioningSetDateTimeResponses::InternalError
            }
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
            let data = match field.bytes().await {
                Ok(d) => d,
                Err(_) => return PutClientBundleResponses::BadRequest,
            };
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
