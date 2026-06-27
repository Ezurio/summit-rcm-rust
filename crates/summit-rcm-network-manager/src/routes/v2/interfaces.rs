 //
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! NetworkManager-owned network interface endpoints.

use crate::routes::shared::DhcpLeasesResponse;
use crate::routes::v2::types::NetworkInterfaceResponse;
use crate::service::{InterfaceError, NetworkService};
use summit_rcm_web::axum::extract::Path;
use log::error;
use summit_rcm_web::serde_json;

summit_rcm_web::define_ok_internal_json_response_family! {
    pub(crate) enum ListInterfacesResponses(Vec<String>);
}

summit_rcm_web::define_json_response_family! {
    pub(crate) enum GetInterfaceResponses {
        Ok(NetworkInterfaceResponse) => 200;
        BadRequest => 400,
        NotFound => 404,
        InternalError => 500
    }
    from NetworkInterfaceResponse => Ok;
}

summit_rcm_web::define_ok_bad_request_internal_json_response_family! {
    pub(crate) enum GetInterfaceDhcpLeasesResponses(DhcpLeasesResponse);
}

#[cfg_attr(feature = "api-docs", utoipa::path(
    get,
    path = "/api/v2/network/interfaces",
    tag = "network",
    responses(ListInterfacesResponses)
))]
pub(crate) async fn list_interfaces() -> ListInterfacesResponses {
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
pub(crate) async fn get_interface(Path(name): Path<String>) -> GetInterfaceResponses {
    match NetworkService::get_interface(&name).await {
        Ok(value) => match serde_json::from_value::<NetworkInterfaceResponse>(value) {
            Ok(value) => value.into(),
            Err(error) => {
                error!("get_interface {} decode: {}", name, error);
                GetInterfaceResponses::InternalError
            }
        },
        Err(InterfaceError::InvalidName) => GetInterfaceResponses::BadRequest,
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
pub(crate) async fn get_interface_dhcp_leases(Path(name): Path<String>) -> GetInterfaceDhcpLeasesResponses {
    match NetworkService::get_dhcp_leases(&name).await {
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
