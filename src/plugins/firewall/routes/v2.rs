//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use crate::plugins::firewall::service::{
    ADD_PORT, REMOVE_PORT, FirewallService, ForwardedPort, IP_VERSIONS,
};
use axum::Json;
use log::error;

#[cfg(feature = "api-docs")]
pub(crate) use super::v2_openapi::ApiDoc;

crate::define_json_response_family! {
    pub enum GetFirewallResponses {
        Ok(Vec<ForwardedPort>) => 200;
    }
    from Vec<ForwardedPort> => Ok;
}

crate::define_json_response_family! {
    pub enum PutFirewallResponses {
        Ok(Vec<ForwardedPort>) => 200;
        InternalError => 500
    }
    from Vec<ForwardedPort> => Ok;
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/firewall/forwardedPorts",
    tag = "firewall",
    responses(GetFirewallResponses)
))]
pub async fn get_firewall() -> GetFirewallResponses {
    FirewallService::get_forwarded_ports().into()
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
    for fp in &desired {
        if !FirewallService::port_is_present(fp) {
            let (ok, msg) = FirewallService::configure_forwarded_port(ADD_PORT, fp.clone()).await;
            if !ok {
                error!("Failed to add forwarded port: {}", msg);
                return PutFirewallResponses::InternalError;
            }
        }
    }
    let current = FirewallService::get_forwarded_ports();
    for fp in current {
        if !desired.contains(&fp) {
            FirewallService::configure_forwarded_port(REMOVE_PORT, fp).await;
        }
    }
    FirewallService::get_forwarded_ports().into()
}
