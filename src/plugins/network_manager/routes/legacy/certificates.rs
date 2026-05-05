//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::certificates::CertificatesService;
use crate::plugins::files::FilesService;
use crate::plugins::network_manager::routes::v2::certificates::CertificateInfo;
use crate::web::legacy_response::SdcerrCode;
use axum::extract::Query;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

crate::define_json_response_family! {
    pub enum GetCertificatesLegacyResponses {
        Ok(LegacyCertificateInfoResponse) => 200;
    }
    from LegacyCertificateInfoResponse => Ok;
}

#[derive(Deserialize)]
pub struct CertificateInfoQuery {
    pub name: Option<String>,
    pub password: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum LegacyCertificateInfoField {
    Detailed(CertificateInfo),
    Empty(BTreeMap<String, String>),
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyCertificateInfoResponse {
    #[serde(rename = "SDCERR")]
    pub sdcerr: i32,
    #[serde(rename = "InfoMsg")]
    pub info_msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_info: Option<LegacyCertificateInfoField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/certificates",
    tag = "legacy",
    responses(GetCertificatesLegacyResponses)
))]
pub async fn get_certificates_legacy(Query(q): Query<CertificateInfoQuery>) -> GetCertificatesLegacyResponses {
    if let Some(name) = q.name.as_deref().filter(|name| !name.is_empty()) {
        return match CertificatesService::get_cert_info(name, q.password.as_deref()).await {
            Ok(info) => match serde_json::from_value::<CertificateInfo>(info) {
                Ok(cert_info) => LegacyCertificateInfoResponse {
                    sdcerr: SdcerrCode::Success.as_i32(),
                    info_msg: String::new(),
                    cert_info: Some(LegacyCertificateInfoField::Detailed(cert_info)),
                    files: None,
                    count: None,
                }
                .into(),
                Err(error) => LegacyCertificateInfoResponse {
                    sdcerr: SdcerrCode::Fail.as_i32(),
                    info_msg: format!("Invalid certificate info shape: {}", error),
                    cert_info: None,
                    files: None,
                    count: None,
                }
                .into(),
            },
            Err(error) => LegacyCertificateInfoResponse {
                sdcerr: SdcerrCode::Fail.as_i32(),
                info_msg: error.to_string(),
                cert_info: if error.to_string().starts_with("Cannot find certificate with name ") {
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

    match FilesService::try_list_files("cert") {
        Ok(certs) => LegacyCertificateInfoResponse {
            sdcerr: SdcerrCode::Success.as_i32(),
            info_msg: "cert files".to_string(),
            cert_info: None,
            files: Some(certs.clone()),
            count: Some(certs.len()),
        }
        .into(),
        Err(_) => LegacyCertificateInfoResponse {
            sdcerr: SdcerrCode::Fail.as_i32(),
            info_msg: "Could not read certificate info".to_string(),
            cert_info: None,
            files: None,
            count: None,
        }
        .into(),
    }
}