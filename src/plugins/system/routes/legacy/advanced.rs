//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use crate::web::legacy_response::SdcerrCode;
use crate::web::legacy_response::LegacyOperationResponse;
use crate::plugins::system::{FACTORY_RESET_SCRIPT, PowerState, SystemService};
use log::error;

crate::define_json_response_family! {
    pub enum LegacyPowerActionResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

crate::define_json_response_family! {
    pub enum FactoryResetLegacyResponses {
        Ok(LegacyOperationResponse) => 200;
    }
    from LegacyOperationResponse => Ok;
}

fn legacy_operation_response(sdcerr: i32, info_msg: impl Into<String>) -> LegacyOperationResponse {
    LegacyOperationResponse { sdcerr, info_msg: info_msg.into() }
}

async fn power_action(state: PowerState, name: &str) -> LegacyPowerActionResponses {
    if let Err(e) = SystemService::set_power_state(state).await {
        error!("{} cannot be initiated: {}", name, e);
        legacy_operation_response(SdcerrCode::Fail.as_i32(), format!("{} cannot be initiated", name)).into()
    } else {
        legacy_operation_response(SdcerrCode::Success.as_i32(), format!("{} initiated", name)).into()
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/poweroff",
    tag = "legacy",
    responses(LegacyPowerActionResponses)
))]
pub async fn poweroff_legacy() -> LegacyPowerActionResponses { power_action(PowerState::Off, "Poweroff").await }

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/suspend",
    tag = "legacy",
    responses(LegacyPowerActionResponses)
))]
pub async fn suspend_legacy() -> LegacyPowerActionResponses { power_action(PowerState::Suspend, "Suspend").await }

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/reboot",
    tag = "legacy",
    responses(LegacyPowerActionResponses)
))]
pub async fn reboot_legacy() -> LegacyPowerActionResponses { power_action(PowerState::Reboot, "Reboot").await }

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/factoryReset",
    tag = "legacy",
    responses(FactoryResetLegacyResponses)
))]
pub async fn factory_reset_legacy() -> FactoryResetLegacyResponses {
    if !std::path::Path::new(FACTORY_RESET_SCRIPT).exists() {
        return legacy_operation_response(
            SdcerrCode::Fail.as_i32(),
            "FactoryReset cannot be initiated - not available on non-encrypted file system images",
        )
        .into();
    }
    match SystemService::initiate_factory_reset().await {
        0 => legacy_operation_response(SdcerrCode::Success.as_i32(), "Reboot required").into(),
        code => legacy_operation_response(SdcerrCode::Fail.as_i32(), format!("Error running factory reset (code {})", code)).into(),
    }
}

