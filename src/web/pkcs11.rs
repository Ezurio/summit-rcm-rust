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
const PKCS11_PROVIDER_URI_DESCRIPTION: &str = "PKCS#11 Provider URI v1.0";

fn der_encode_length(length: usize) -> ([u8; 5], usize) {
    let mut encoded = [0_u8; 5];
    if length < 0x80 {
        encoded[0] = length as u8;
        return (encoded, 1);
    }

    let byte_count = if length <= 0xff {
        1
    } else if length <= 0xffff {
        2
    } else if length <= 0xff_ffff {
        3
    } else {
        4
    };

    encoded[0] = 0x80 | byte_count as u8;
    for index in 0..byte_count {
        let shift = (byte_count - index - 1) * 8;
        encoded[index + 1] = (length >> shift) as u8;
    }

    (encoded, byte_count + 1)
}

fn append_der_length(output: &mut Vec<u8>, length: usize) {
    let (encoded, encoded_len) = der_encode_length(length);
    output.extend_from_slice(&encoded[..encoded_len]);
}

fn der_encode_tagged_string(output: &mut Vec<u8>, tag: u8, value: &str) {
    let bytes = value.as_bytes();
    output.push(tag);
    append_der_length(output, bytes.len());
    output.extend_from_slice(bytes);
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

    let desc_len = 1 + der_encode_length(PKCS11_PROVIDER_URI_DESCRIPTION.len()).1
        + PKCS11_PROVIDER_URI_DESCRIPTION.len();
    let uri_len = 1 + der_encode_length(pkcs11_uri.len()).1 + pkcs11_uri.len();
    let mut der = Vec::with_capacity(1 + 5 + desc_len + uri_len);
    der.push(0x30);
    append_der_length(&mut der, desc_len + uri_len);
    der_encode_tagged_string(&mut der, 0x1a, PKCS11_PROVIDER_URI_DESCRIPTION);
    der_encode_tagged_string(&mut der, 0x0c, pkcs11_uri);

    let body = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::from("-----BEGIN PKCS#11 PROVIDER URI-----\n");
    for chunk in body.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).expect("base64 output is ascii"));
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