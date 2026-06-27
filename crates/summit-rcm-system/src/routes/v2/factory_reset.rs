//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! POST /api/v2/system/factoryReset

use crate::routes::shared::{self, FactoryResetResult};
use serde::Serialize;
use log::error;

summit_rcm_web::define_ok_internal_json_response_family! {
    pub enum FactoryResetResponses(FactoryResetResponse);
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub(crate) struct FactoryResetResponse {
    pub result: String,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/system/factoryReset",
    tag = "system",
    responses(FactoryResetResponses)
))]
pub(crate) async fn factory_reset() -> FactoryResetResponses {
    match shared::run_factory_reset().await {
        FactoryResetResult::Initiated => FactoryResetResponse {
            result: "ok".to_string(),
        }
        .into(),
        FactoryResetResult::NotAvailable => {
            error!("factory_reset unavailable on this image");
            FactoryResetResponses::InternalError
        }
        FactoryResetResult::Failed(code) => {
            error!("factory_reset returned: {}", code);
            FactoryResetResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/factoryReset",
    tag = "system",
    responses(FactoryResetResponses)
))]
pub(crate) async fn factory_reset_put() -> FactoryResetResponses {
    factory_reset().await
}
