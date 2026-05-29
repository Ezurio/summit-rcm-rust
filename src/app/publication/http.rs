//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub use summit_rcm_plugin_api::{PublishedRoute, RouteAuthPolicy, RouteMode};
#[cfg(feature = "api-docs")]
pub use summit_rcm_plugin_api::RouteDocPolicy;

use axum::Router;

use super::CapabilityPublication;

pub type RouteInstaller = fn(Router) -> Router;
type RouteMetadata = &'static [PublishedRoute];

#[derive(Clone, Copy, Debug)]
pub struct RoutePublication {
    pub common: CapabilityPublication<RouteMetadata, RouteInstaller>,
    pub auth: RouteAuthPolicy,
    pub mode: RouteMode,
}

impl RoutePublication {
    pub const fn new(routes: RouteMetadata, install: RouteInstaller, auth: RouteAuthPolicy) -> Self {
        Self {
            common: CapabilityPublication::new(routes, install),
            auth,
            mode: RouteMode::NormalOnly,
        }
    }

    pub const fn new_with_mode(
        routes: RouteMetadata,
        install: RouteInstaller,
        auth: RouteAuthPolicy,
        mode: RouteMode,
    ) -> Self {
        Self {
            common: CapabilityPublication::new(routes, install),
            auth,
            mode,
        }
    }

    pub const fn install_only(install: RouteInstaller, auth: RouteAuthPolicy) -> Self {
        Self {
            common: CapabilityPublication::install_only(install),
            auth,
            mode: RouteMode::NormalOnly,
        }
    }
}

#[cfg(feature = "api-docs")]
pub const fn route_doc_policies_len(route_publications: &[RoutePublication]) -> usize {
    let mut len = 0;
    let mut index = 0;

    while index < route_publications.len() {
        if let Some(routes) = route_publications[index].common.metadata
            && !routes.is_empty()
        {
            len += 1;
        }
        index += 1;
    }

    len
}

#[cfg(feature = "api-docs")]
pub const fn derive_route_doc_policies<const N: usize>(
    route_publications: &[RoutePublication],
) -> [RouteDocPolicy; N] {
    let mut policies = [RouteDocPolicy::new("", RouteAuthPolicy::UnauthenticatedAllowed); N];
    let mut route_index = 0;
    let mut policy_index = 0;

    while route_index < route_publications.len() {
        let publication = &route_publications[route_index];
        if let Some(routes) = publication.common.metadata
            && !routes.is_empty()
        {
            policies[policy_index] = RouteDocPolicy::new(routes[0].path, publication.auth);
            policy_index += 1;
        }
        route_index += 1;
    }

    policies
}

const _: fn(&'static str, &'static str) -> PublishedRoute = PublishedRoute::new;
const _: fn(String, String) -> PublishedRoute = PublishedRoute::leak;
const _: fn(RouteMetadata, RouteInstaller, RouteAuthPolicy) -> RoutePublication = RoutePublication::new;
const _: fn(RouteMetadata, RouteInstaller, RouteAuthPolicy, RouteMode) -> RoutePublication = RoutePublication::new_with_mode;
const _: fn(RouteInstaller, RouteAuthPolicy) -> RoutePublication = RoutePublication::install_only;
#[cfg(feature = "api-docs")]
const _: fn(&[RoutePublication]) -> usize = route_doc_policies_len;