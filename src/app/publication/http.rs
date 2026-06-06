//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub use summit_rcm_plugin_api::{PublishedRoute, RouteAuthPolicy, RouteMode};
#[cfg(feature = "api-docs")]
pub use summit_rcm_plugin_api::RouteDocPolicy;

use axum::Router;

pub type RouteInstaller = fn(Router) -> Router;

#[derive(Clone, Copy, Debug)]
pub struct RoutePublication {
    pub routes: &'static [PublishedRoute],
    pub install: RouteInstaller,
    pub auth: RouteAuthPolicy,
    pub mode: RouteMode,
}

#[cfg(feature = "api-docs")]
pub const fn route_doc_policies_len(route_publications: &[RoutePublication]) -> usize {
    let mut len = 0;
    let mut index = 0;

    while index < route_publications.len() {
        if !route_publications[index].routes.is_empty() {
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
        if !publication.routes.is_empty() {
            policies[policy_index] = RouteDocPolicy::new(publication.routes[0].path, publication.auth);
            policy_index += 1;
        }
        route_index += 1;
    }

    policies
}

const _: fn(&'static str, &'static str) -> PublishedRoute = PublishedRoute::new;
const _: fn(String, String) -> PublishedRoute = PublishedRoute::leak;
#[cfg(feature = "api-docs")]
const _: fn(&[RoutePublication]) -> usize = route_doc_policies_len;