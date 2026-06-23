//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use summit_rcm_web::legacy_response::{fail_response, ok_response, LegacyOperationResponse};
use crate::service::{FirewallService, ForwardedPort};
use summit_rcm_web::axum::{extract::Path, Json};
use serde::{Deserialize, Serialize};

#[cfg(feature = "api-docs")]
pub(crate) use super::legacy_openapi::ApiDoc;

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyForwardedPortResponse {
    pub port: u16,
    pub protocol: String,
    pub toport: String,
    pub toaddr: String,
    pub ip_version: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyFirewallListResponse {
    #[serde(flatten)]
    pub operation: LegacyOperationResponse,
    #[serde(rename = "Forward")]
    pub forward: Vec<LegacyForwardedPortResponse>,
}

summit_rcm_web::define_ok_json_response_family! {
    pub enum GetFirewallLegacyResponses(LegacyFirewallListResponse);
}

pub type PutFirewallLegacyResponses = summit_rcm_web::legacy_response::LegacyOperationOkResponse;

fn legacy_forwarded_port_response(port: ForwardedPort) -> LegacyForwardedPortResponse {
    LegacyForwardedPortResponse {
        port: port.port,
        protocol: port.protocol,
        toport: port.toport,
        toaddr: port.toaddr,
        ip_version: port.ip_version,
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/firewall",
    tag = "firewall",
    responses(GetFirewallLegacyResponses)
))]
pub async fn get_firewall_legacy() -> GetFirewallLegacyResponses {
    let ports = FirewallService::get_forwarded_ports()
        .await
        .into_iter()
        .map(legacy_forwarded_port_response)
        .collect();
    LegacyFirewallListResponse {
        operation: ok_response(""),
        forward: ports,
    }
    .into()
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/firewall/{command}",
    tag = "firewall",
    params(("command" = String, Path, description = "Firewall command")),
    responses(GetFirewallLegacyResponses)
))]
pub async fn get_firewall_legacy_with_command(Path(_command): Path<String>) -> GetFirewallLegacyResponses {
    get_firewall_legacy().await
}

#[derive(Deserialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct LegacyForwardedPort {
    pub port: u16,
    pub protocol: String,
    pub toport: String,
    pub toaddr: String,
    pub ip_version: Option<String>,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/firewall/{command}",
    tag = "firewall",
    params(("command" = String, Path, description = "Firewall command")),
    request_body = LegacyForwardedPort,
    responses(PutFirewallLegacyResponses)
))]
pub async fn put_firewall_legacy(
    Path(command): Path<String>,
    Json(body): Json<LegacyForwardedPort>,
) -> PutFirewallLegacyResponses {
    use crate::service::{IPV4, PORT_COMMANDS, IP_VERSIONS};

    if !PORT_COMMANDS.contains(&command.as_str()) {
        return fail_response(format!(
            "supplied parameter 'command' value {} must be one of ['addForwardPort', 'removeForwardPort'], ",
            command
        ))
        .into();
    }

    let ip_ver = body.ip_version.as_deref().unwrap_or(IPV4).to_string();
    if !IP_VERSIONS.contains(&ip_ver.as_str()) {
        return fail_response(format!("invalid ip_version: {}", ip_ver)).into();
    }

    let fp = ForwardedPort {
        port: body.port,
        protocol: body.protocol,
        toport: body.toport,
        toaddr: body.toaddr,
        ip_version: ip_ver,
    };

    let (ok, msg) = FirewallService::configure_forwarded_port(&command, fp).await;
    if ok {
        ok_response(msg).into()
    } else {
        fail_response(msg).into()
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    put,
    path = "/firewall",
    tag = "firewall",
    request_body = LegacyForwardedPort,
    responses(PutFirewallLegacyResponses)
))]
pub async fn put_firewall_legacy_default(Json(_body): Json<LegacyForwardedPort>) -> PutFirewallLegacyResponses {
    fail_response("No command specified").into()
}
