//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::certificates::{CertificateInfo, CertificatesService};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use summit_rcm_core::files_service::FilesService;
use summit_rcm_web::axum::extract::Query;
use summit_rcm_web::legacy_response::{LegacyOperationResponse, fail_response, ok_response};

summit_rcm_web::define_ok_json_response_family! {
    pub(crate) enum GetCertificatesLegacyResponses(LegacyCertificateInfoResponse);
}

#[derive(Deserialize)]
pub(crate) struct CertificateInfoQuery {
    pub name: Option<String>,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub(crate) enum LegacyCertificateInfoField {
    Detailed(CertificateInfo),
    Empty(BTreeMap<String, String>),
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct LegacyCertificateInfoResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_info: Option<LegacyCertificateInfoField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

#[cfg_attr(
    feature = "api-docs",
    utoipa::path(
        get,
        path = "/certificates",
        tag = "legacy",
        responses(GetCertificatesLegacyResponses)
    )
)]
pub(crate) async fn get_certificates_legacy(
    Query(q): Query<CertificateInfoQuery>,
) -> GetCertificatesLegacyResponses {
    if let Some(name) = q.name.as_deref().filter(|name| !name.is_empty()) {
        return match CertificatesService::get_cert_info_model(name, q.password.as_deref()).await {
            Ok(cert_info) => LegacyCertificateInfoResponse {
                operation: ok_response(""),
                cert_info: Some(LegacyCertificateInfoField::Detailed(cert_info)),
                files: None,
                count: None,
            }
            .into(),
            Err(error) => LegacyCertificateInfoResponse {
                operation: fail_response(error.to_string()),
                cert_info: if error
                    .to_string()
                    .starts_with("Cannot find certificate with name ")
                {
                    Some(LegacyCertificateInfoField::Empty(BTreeMap::new()))
                } else {
                    None
                },
                files: None,
                count: None,
            }
            .into(),
        };
    }

    match FilesService::try_list_files("cert").await {
        Ok(certs) => LegacyCertificateInfoResponse {
            operation: ok_response("cert files"),
            cert_info: None,
            files: Some(certs.clone()),
            count: Some(certs.len()),
        }
        .into(),
        Err(_) => LegacyCertificateInfoResponse {
            operation: fail_response("Could not read certificate info"),
            cert_info: None,
            files: None,
            count: None,
        }
        .into(),
    }
}
