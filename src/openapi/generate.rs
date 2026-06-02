// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
// Host tool: write the summit-rcm OpenAPI JSON to SUMMIT_RCM_OPENAPI_OUTPUT.

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let output_path = std::env::var("SUMMIT_RCM_OPENAPI_OUTPUT")?;
    tokio::fs::write(output_path, serde_json::to_string(&*summit_rcm::web::openapi::OPENAPI_DOC)?).await?;
    Ok(())
}
