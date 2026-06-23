//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
use summit_rcm_web::legacy_response::{fail_response, ok_response};
use crate::routes::shared::FactoryResetResult;
use crate::{PowerState, SystemService};
use log::error;

pub type LegacyPowerActionResponses = summit_rcm_web::legacy_response::LegacyOperationOkResponse;
pub type FactoryResetLegacyResponses = summit_rcm_web::legacy_response::LegacyOperationOkResponse;

async fn power_action(state: PowerState, name: &str) -> LegacyPowerActionResponses {
    if let Err(error) = SystemService::request_power_state(state).await {
        error!("{} cannot be initiated: {}", name, error);
        fail_response(format!("{} cannot be initiated", name)).into()
    } else {
        ok_response(format!("{} initiated", name)).into()
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
    match crate::routes::shared::run_factory_reset().await {
        FactoryResetResult::NotAvailable => fail_response(
            "FactoryReset cannot be initiated - not available on non-encrypted file system images",
        )
        .into(),
        FactoryResetResult::Initiated => ok_response("Reboot required").into(),
        FactoryResetResult::Failed(code) => {
            fail_response(format!("Error running factory reset (code {})", code)).into()
        }
    }
}
