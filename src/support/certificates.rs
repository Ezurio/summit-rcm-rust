//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Shared certificate utilities.

use anyhow::Result;
use crate::utils::path_exists;
use openssl::pkcs12::Pkcs12;
use openssl::x509::{X509, X509NameRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct CertificateExtension {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct CertificateInfo {
    pub version: i32,
    pub serial_number: String,
    pub subject: String,
    pub issuer: String,
    pub not_before: String,
    pub not_after: String,
    pub extensions: Vec<CertificateExtension>,
}

pub struct CertificatesService;

impl CertificatesService {
    fn format_x509_name(name: &X509NameRef) -> String {
        let mut formatted = String::new();
        for entry in name.entries() {
            formatted.push('/');
            formatted.push_str(entry.object().nid().short_name().unwrap_or("UNKNOWN"));
            formatted.push('=');

            let value = entry
                .data()
                .as_utf8()
                .map(|value| value.to_string())
                .unwrap_or_else(|_| String::from_utf8_lossy(entry.data().as_slice()).into_owned());
            formatted.push_str(&value);
        }
        formatted
    }

    fn parse_extensions(cert: &X509) -> Result<Vec<CertificateExtension>> {
        let text = String::from_utf8(cert.to_text()?)?;
        let mut extensions = Vec::new();
        let mut in_extensions = false;
        let mut current_name: Option<String> = None;
        let mut current_value: Vec<String> = Vec::new();

        for line in text.lines() {
            if !in_extensions {
                if line.trim() == "X509v3 extensions:" {
                    in_extensions = true;
                }
                continue;
            }

            if !line.starts_with(' ') {
                break;
            }

            let trimmed = line.trim_end();
            let is_header = line.starts_with("            ") && !line.starts_with("                ");
            let is_value = line.starts_with("                ");

            if is_header {
                if let Some(name) = current_name.take() {
                    extensions.push(CertificateExtension {
                        name,
                        value: current_value.join("\n"),
                    });
                    current_value.clear();
                }

                let header = trimmed.trim();
                let name = header
                    .split_once(':')
                    .map(|(name, _)| name.trim())
                    .unwrap_or(header)
                    .to_string();
                current_name = Some(name);
            } else if is_value {
                current_value.push(trimmed.trim().to_string());
            }
        }

        if let Some(name) = current_name.take() {
            extensions.push(CertificateExtension {
                name,
                value: current_value.join("\n"),
            });
        }

        Ok(extensions)
    }

    fn build_cert_info(cert: &X509) -> Result<CertificateInfo> {
        Ok(CertificateInfo {
            version: cert.version() + 1,
            serial_number: cert.serial_number().to_bn()?.to_dec_str()?.to_string(),
            subject: Self::format_x509_name(cert.subject_name()),
            issuer: Self::format_x509_name(cert.issuer_name()),
            not_before: cert.not_before().to_string(),
            not_after: cert.not_after().to_string(),
            extensions: Self::parse_extensions(cert)?,
        })
    }

    pub fn parse_certificate_bytes(data: &[u8], password: Option<&str>) -> Result<X509> {
        if let Ok(cert) = X509::from_pem(data) {
            return Ok(cert);
        }

        if let Ok(cert) = X509::from_der(data) {
            return Ok(cert);
        }

        if let Ok(certs) = X509::stack_from_pem(data)
            && let Some(cert) = certs.into_iter().next() {
                return Ok(cert);
            }

        let pkcs12 = Pkcs12::from_der(data)?;
        let parsed = pkcs12.parse2(password.unwrap_or(""))?;
        parsed
            .cert
            .ok_or_else(|| anyhow::anyhow!("unable to parse certificate"))
    }

    async fn load_cert(cert_name: &str, password: Option<&str>) -> Result<CertificateInfo> {
        let safe_name = Path::new(cert_name)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid certificate name: {}", cert_name))?;
        let cert_path = format!("{}{}", crate::definition::NETWORKMANAGER_CERT_DIR, safe_name);
        if !path_exists(&cert_path).await {
            return Err(anyhow::anyhow!("Cannot find certificate with name {}", safe_name));
        }
        let data = tokio::fs::read(&cert_path).await?;
        let cert = Self::parse_certificate_bytes(&data, password)?;
        Self::build_cert_info(&cert)
    }

    /// Return metadata about a certificate file using the OpenSSL library.
    pub async fn get_cert_info(cert_name: &str, password: Option<&str>) -> Result<Value> {
        let info = Self::load_cert(cert_name, password).await?;
        serde_json::to_value(&info).map_err(Into::into)
    }

    pub async fn get_cert_info_model(cert_name: &str, password: Option<&str>) -> Result<CertificateInfo> {
        Self::load_cert(cert_name, password).await
    }
}
