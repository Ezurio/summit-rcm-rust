 //
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! NetworkManager-owned network interface endpoints.

use crate::plugins::network_manager::routes::shared::{DhcpLeasesResponse, NetworkInterfaceResponse};
use crate::plugins::network_manager::service::{InterfaceError, NetworkService};
use axum::extract::Path;
use log::error;

crate::define_ok_internal_json_response_family! {
    pub enum ListInterfacesResponses(Vec<String>);
}

crate::define_ok_not_found_internal_json_response_family! {
    pub enum GetInterfaceResponses(NetworkInterfaceResponse);
}

crate::define_ok_bad_request_internal_json_response_family! {
    pub enum GetInterfaceDhcpLeasesResponses(DhcpLeasesResponse);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces",
    tag = "network",
    responses(ListInterfacesResponses)
))]
pub async fn list_interfaces() -> ListInterfacesResponses {
    match NetworkService::get_all_interfaces_model().await {
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
                error!("get_interface {} decode: {}", name, error);
                GetInterfaceResponses::InternalError
            }
        },
        Err(InterfaceError::NotFound) => GetInterfaceResponses::NotFound,
        Err(error) => {
            error!("get_interface {}: {:?}", name, error);
            GetInterfaceResponses::InternalError
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
                error!("get_interface_dhcp_leases {} decode: {}", name, error);
                GetInterfaceDhcpLeasesResponses::InternalError
            }
        },
        Err(InterfaceError::InvalidName) => {
            GetInterfaceDhcpLeasesResponses::BadRequest
        }
        Err(error) => {
            error!("get_interface_dhcp_leases {}: {:?}", name, error);
            GetInterfaceDhcpLeasesResponses::InternalError
        }
    }
}
