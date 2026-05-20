 //
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! NetworkManager-owned network interface endpoints.

use crate::plugins::network_manager::routes::shared::NetworkInterfaceResponse;
use crate::plugins::network_manager::service::NetworkService;
use axum::extract::Path;
use serde::{Deserialize, Serialize};
use log::error;

crate::define_json_response_family! {
    pub enum ListInterfacesResponses {
        Ok(Vec<String>) => 200;
        InternalError => 500
    }
    from Vec<String> => Ok;
}

crate::define_json_response_family! {
    pub enum GetInterfaceResponses {
        Ok(NetworkInterfaceResponse) => 200;
        NotFound => 404,
        InternalError => 500
    }
    from NetworkInterfaceResponse => Ok;
}

crate::define_json_response_family! {
    pub enum GetInterfaceDhcpLeasesResponses {
        Ok(DhcpLeasesResponse) => 200;
        BadRequest => 400,
        InternalError => 500
    }
    from DhcpLeasesResponse => Ok;
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Ipv4DhcpLease {
    pub expiry: i64,
    pub mac_address: String,
    pub ip_address: String,
    pub hostname: String,
    pub client_identifier: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Ipv6DhcpLease {
    pub expiry: i64,
    pub iaid: String,
    pub ip_address: String,
    pub hostname: String,
    pub client_duid: String,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
pub struct DhcpLeasesResponse {
    pub ipv4: Vec<Ipv4DhcpLease>,
    pub ipv6: Vec<Ipv6DhcpLease>,
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces",
    tag = "network",
    responses(ListInterfacesResponses)
))]
pub async fn list_interfaces() -> ListInterfacesResponses {
    match NetworkService::get_all_interfaces()
        .await
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).map_err(Into::into))
    {
        Ok(value) => value.into(),
        Err(error) => {
            log::error!("list_interfaces: {}", error);
            ListInterfacesResponses::InternalError
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces/{name}",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(GetInterfaceResponses)
))]
pub async fn get_interface(Path(name): Path<String>) -> GetInterfaceResponses {
    match NetworkService::get_interface(&name).await {
        Ok(value) => match serde_json::from_value::<NetworkInterfaceResponse>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                error!("get_interface {} invalid response shape: {}", name, error);
                GetInterfaceResponses::InternalError
            }
        },
        Err(error) => {
            error!("get_interface {}: {}", name, error);
            if error.to_string().contains("not found") {
                GetInterfaceResponses::NotFound
            } else {
                GetInterfaceResponses::InternalError
            }
        }
    }
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces/{name}/dhcpLeases",
    tag = "network",
    params(("name" = String, Path, description = "Interface name")),
    responses(GetInterfaceDhcpLeasesResponses)
))]
pub async fn get_interface_dhcp_leases(Path(name): Path<String>) -> GetInterfaceDhcpLeasesResponses {
    match NetworkService::get_dhcp_leases(&name) {
        Ok(value) => match serde_json::from_value::<DhcpLeasesResponse>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                error!("get_interface_dhcp_leases {} invalid response shape: {}", name, error);
                GetInterfaceDhcpLeasesResponses::InternalError
            }
        },
        Err(error) => {
            error!("get_interface_dhcp_leases {}: {}", name, error);
            if error.to_string().contains("Invalid interface")
                || error.to_string().contains("No interface")
            {
                GetInterfaceDhcpLeasesResponses::BadRequest
            } else {
                GetInterfaceDhcpLeasesResponses::InternalError
            }
        }
    }
}
