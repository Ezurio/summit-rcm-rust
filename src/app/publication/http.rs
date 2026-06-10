//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedRoute {
    pub method: &'static str,
    pub path: &'static str,
}

impl PublishedRoute {
    pub const fn new(method: &'static str, path: &'static str) -> Self {
        Self { method, path }
    }

    pub fn leak(method: String, path: String) -> Self {
        Self {
            method: Box::leak(method.into_boxed_str()),
            path: Box::leak(path.into_boxed_str()),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteAuthPolicy {
    SessionRequired = 0,
    UnauthenticatedAllowed = 1,
}

/// Boot-mode visibility for a route.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteMode {
    NormalOnly = 0,
    ProvisioningOnly = 1,
    Any = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteDocPolicy {
    pub path: &'static str,
    pub auth: RouteAuthPolicy,
}

impl RouteDocPolicy {
    pub const fn new(path: &'static str, auth: RouteAuthPolicy) -> Self {
        Self { path, auth }
    }
}

#[cfg(feature = "api-docs")]
pub type OpenApiJsonFn = fn() -> String;

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