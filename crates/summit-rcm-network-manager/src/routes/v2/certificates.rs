//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Network certificate endpoints.

use crate::certificates::{CertificateInfo, CertificatesService};
use log::error;
use serde::Deserialize;
use summit_rcm_core::files_service::FilesService;
use summit_rcm_web::axum;
use summit_rcm_web::axum::{
    body::to_bytes,
    extract::{Multipart, Path, multipart::MultipartRejection},
};
use summit_rcm_web::serde_json;

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum ListCertificatesResponses(Vec<String>);
}

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
    pub(crate) enum GetCertificateResponses(CertificateInfo);
}

summit_rcm_web::define_status_response_family! {
    pub(crate) enum UploadCertificateResponses {
        Ok => 200,
        BadRequest => 400,
        InternalError => 500
    }
}

summit_rcm_web::define_status_response_family! {
    pub(crate) enum DeleteCertificateResponses {
        Ok => 200,
        NotFound => 404,
        InternalError => 500
    }
}

#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[derive(Deserialize)]
pub(crate) struct CertificateInfoRequest {
    pub password: Option<String>,
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/api/v2/network/certificates",
        tag = "network",
        responses(ListCertificatesResponses)
    )
)]
pub(crate) async fn list_certificates() -> ListCertificatesResponses {
    match FilesService::try_get_cert_and_pac_files().await {
        Ok(files) => files.into(),
        Err(error) => {
            error!("list_certificates: {}", error);
            ListCertificatesResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/certificates/{name}",
    tag = "network",
    params(("name" = String, Path, description = "Certificate name")),
    request_body = CertificateInfoRequest,
    responses(GetCertificateResponses)
))]
pub(crate) async fn get_certificate(
    Path(name): Path<String>,
    req: axum::extract::Request,
) -> GetCertificateResponses {
    let body = match to_bytes(req.into_body(), 64 * 1024).await {
        Ok(body) => body,
        Err(error) => {
            error!("get_certificate {}: {}", name, error);
            return GetCertificateResponses::BadRequest;
        }
    };

    let password = if body.is_empty() {
        None
    } else {
        match serde_json::from_slice::<CertificateInfoRequest>(&body) {
            Ok(payload) => payload.password,
            Err(error) => {
                error!("get_certificate {} invalid request body: {}", name, error);
                return GetCertificateResponses::BadRequest;
            }
        }
    };

    match CertificatesService::get_cert_info_model(&name, password.as_deref()).await {
        Ok(cert_info) => cert_info.into(),
        Err(error) => {
            error!("get_certificate {}: {}", name, error);
            GetCertificateResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/network/certificates/{name}",
    tag = "network",
    params(("name" = String, Path, description = "Certificate name")),
    request_body(content = String, content_type = "multipart/form-data"),
    responses(UploadCertificateResponses)
))]
pub(crate) async fn upload_certificate(
    Path(name): Path<String>,
    multipart: Result<Multipart, MultipartRejection>,
) -> UploadCertificateResponses {
    let Ok(mut multipart) = multipart else {
        return UploadCertificateResponses::BadRequest;
    };

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() != Some("file") {
            continue;
        }

        let data = match field.bytes().await {
            Ok(data) => data,
            Err(error) => {
                error!("upload_certificate {} read field: {}", name, error);
                return UploadCertificateResponses::BadRequest;
            }
        };

        return match FilesService::upload_file("cert", &name, &data).await {
            Ok(()) => UploadCertificateResponses::Ok,
            Err(error) => {
                error!("upload_certificate {}: {}", name, error);
                UploadCertificateResponses::InternalError
            }
        };
    }

    UploadCertificateResponses::BadRequest
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    delete,
    path = "/api/v2/network/certificates/{name}",
    tag = "network",
    params(("name" = String, Path, description = "Certificate name")),
    responses(DeleteCertificateResponses)
))]
pub(crate) async fn delete_certificate(Path(name): Path<String>) -> DeleteCertificateResponses {
    match FilesService::delete_file_typed("cert", &name).await {
        Ok(()) => DeleteCertificateResponses::Ok,
        Err(summit_rcm_core::files_service::FileDeleteError::NotFound) => {
            DeleteCertificateResponses::NotFound
        }
        Err(error) => {
            error!("delete_certificate {}: {:?}", name, error);
            DeleteCertificateResponses::InternalError
        }
    }
}
