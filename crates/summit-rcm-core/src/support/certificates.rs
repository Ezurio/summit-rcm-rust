//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Certificate parsing utility.

use anyhow::Result;
use openssl::pkcs12::Pkcs12;
use openssl::x509::X509;

pub struct CertificatesService;

impl CertificatesService {
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
}
