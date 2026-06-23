//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::service::{
    ADD_PORT, REMOVE_PORT, FirewallService, ForwardedPort, IP_VERSIONS,
};
use summit_rcm_web::axum::Json;
use log::error;
use std::collections::HashSet;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

summit_rcm_web::define_ok_json_response_family! {
    pub enum GetFirewallResponses(Vec<ForwardedPort>);
}

summit_rcm_web::define_ok_internal_json_response_family! {
    pub enum PutFirewallResponses(Vec<ForwardedPort>);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/firewall/forwardedPorts",
    tag = "firewall",
    responses(GetFirewallResponses)
))]
pub async fn get_firewall() -> GetFirewallResponses {
    FirewallService::get_forwarded_ports().await.into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/api/v2/network/firewall/forwardedPorts",
    tag = "firewall",
    responses(PutFirewallResponses)
))]
pub async fn put_firewall(Json(desired): Json<Vec<ForwardedPort>>) -> PutFirewallResponses {
    for fp in &desired {
        if !IP_VERSIONS.contains(&fp.ip_version.as_str()) {
            return PutFirewallResponses::InternalError;
        }
        if fp.protocol != "tcp" && fp.protocol != "udp" {
            return PutFirewallResponses::InternalError;
        }
    }
    let desired_set: HashSet<ForwardedPort> = desired.iter().cloned().collect();
    let current_set: HashSet<ForwardedPort> = FirewallService::get_forwarded_ports().await.into_iter().collect();

    for fp in &desired {
        if !current_set.contains(fp) {
            let (ok, msg) = FirewallService::configure_forwarded_port(ADD_PORT, fp.clone()).await;
            if !ok {
                error!("Failed to add forwarded port: {}", msg);
                return PutFirewallResponses::InternalError;
            }
        }
    }
    for fp in current_set {
        if !desired_set.contains(&fp) {
            let _ = FirewallService::configure_forwarded_port(REMOVE_PORT, fp).await;
        }
    }
    FirewallService::get_forwarded_ports().await.into()
}
