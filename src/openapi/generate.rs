//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[cfg(feature = "api-docs")]
fn main() -> anyhow::Result<()> {
    let output_path = std::env::var("SUMMIT_RCM_OPENAPI_OUTPUT")?;
    std::fs::write(output_path, serde_json::to_string(&*summit_rcm::web::openapi::OPENAPI_DOC)?)?;
    Ok(())
}

#[cfg(not(feature = "api-docs"))]
fn main() {}