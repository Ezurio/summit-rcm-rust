//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! POST /api/v2/system/factoryReset

use crate::plugins::system::SystemService;
use serde::Serialize;
use log::error;

crate::define_json_response_family! {
    pub enum FactoryResetResponses {
        Ok(FactoryResetResponse) => 200;
        InternalError => 500
    }
    from FactoryResetResponse => Ok;
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct FactoryResetResponse {
    pub result: String,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    post,
    path = "/api/v2/system/factoryReset",
    tag = "system",
    responses(FactoryResetResponses)
))]
pub async fn factory_reset() -> FactoryResetResponses {
    let rc = SystemService::initiate_factory_reset().await;
    if rc == 0 {
        FactoryResetResponse {
            result: "ok".to_string(),
        }
        .into()
    } else {
        error!("factory_reset returned: {}", rc);
        FactoryResetResponses::InternalError
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/system/factoryReset",
    tag = "system",
    responses(FactoryResetResponses)
))]
pub async fn factory_reset_put() -> FactoryResetResponses {
    factory_reset().await
}
