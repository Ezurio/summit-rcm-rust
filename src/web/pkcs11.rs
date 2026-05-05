//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::{bail, Result};
use base64::Engine as _;
use std::path::Path;

// uri2pem.py does not emit a real certificate or private-key PEM here; it emits
// a tiny custom ASN.1 DER wrapper understood by pkcs11-provider:
//
//   SEQUENCE {
//     VisibleString("PKCS#11 Provider URI v1.0"),
//     UTF8String(<pkcs11-uri>)
//   }
//
// We encode this DER manually because the structure is trivial, we need exact
// control over the emitted bytes for parity with uri2pem.py, and it avoids
// pulling in a separate ASN.1-building dependency for this one small wrapper.
fn der_encode_length(length: usize) -> Vec<u8> {
    if length < 0x80 {
        return vec![length as u8];
    }

    let bytes = if length <= 0xff {
        vec![length as u8]
    } else if length <= 0xffff {
        vec![(length >> 8) as u8, length as u8]
    } else if length <= 0xff_ffff {
        vec![(length >> 16) as u8, (length >> 8) as u8, length as u8]
    } else {
        vec![
            (length >> 24) as u8,
            (length >> 16) as u8,
            (length >> 8) as u8,
            length as u8,
        ]
    };

    let mut encoded = Vec::with_capacity(bytes.len() + 1);
    encoded.push(0x80 | bytes.len() as u8);
    encoded.extend(bytes);
    encoded
}

fn der_encode_tagged_string(tag: u8, value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut encoded = Vec::with_capacity(1 + 5 + bytes.len());
    encoded.push(tag);
    encoded.extend(der_encode_length(bytes.len()));
    encoded.extend(bytes);
    encoded
}

fn pkcs11_uri_to_pem_bytes(pkcs11_uri: &str) -> Result<Vec<u8>> {
    if !pkcs11_uri.starts_with("pkcs11:") {
        bail!("uri({}) not a valid PKCS#11 URI", pkcs11_uri);
    }
    if !(pkcs11_uri.contains("object=") || pkcs11_uri.contains("id=")) {
        bail!(
            "uri({}) does not specify an object by label or id",
            pkcs11_uri
        );
    }

    let desc = der_encode_tagged_string(0x1a, "PKCS#11 Provider URI v1.0");
    let uri = der_encode_tagged_string(0x0c, pkcs11_uri);

    let mut der = Vec::with_capacity(1 + 5 + desc.len() + uri.len());
    der.push(0x30);
    der.extend(der_encode_length(desc.len() + uri.len()));
    der.extend(desc);
    der.extend(uri);

    let body = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::from("-----BEGIN PKCS#11 PROVIDER URI-----\n");
    for chunk in body.as_bytes().chunks(64) {
        pem.push_str(&String::from_utf8_lossy(chunk));
        pem.push('\n');
    }
    pem.push_str("-----END PKCS#11 PROVIDER URI-----\n");

    Ok(pem.into_bytes())
}

pub async fn convert_pkcs11_uri_to_pem(
    pkcs11_uri: &str,
    output_path: impl AsRef<Path>,
) -> Result<()> {
    let pem = pkcs11_uri_to_pem_bytes(pkcs11_uri)?;
    tokio::fs::write(output_path, pem).await?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/web/pkcs11.rs"]
mod tests;