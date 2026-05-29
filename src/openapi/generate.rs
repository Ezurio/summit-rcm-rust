//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[cfg(feature = "api-docs")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let output_path = std::env::var("SUMMIT_RCM_OPENAPI_OUTPUT")?;
    tokio::fs::write(output_path, serde_json::to_string(&*summit_rcm::web::openapi::OPENAPI_DOC)?).await?;
    Ok(())
}

#[cfg(not(feature = "api-docs"))]
fn main() {}