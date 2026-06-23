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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteAuthPolicy {
    SessionRequired,
    UnauthenticatedAllowed,
}

/// Boot-mode visibility for a route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteMode {
    /// Registered in all boot modes.
    Any,
    /// Registered only when the daemon is fully provisioned.
    FullyProvisioned,
    /// Registered while the daemon is not yet fully provisioned
    /// (`Unprovisioned` or `PartiallyProvisioned`).
    NotFullyProvisioned,
    /// Registered once some provisioning has completed
    /// (`PartiallyProvisioned` or `FullyProvisioned`).
    SomeProvisioning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutePolicy {
    pub auth: RouteAuthPolicy,
    pub mode: RouteMode,
}

impl RoutePolicy {
    pub const fn new(auth: RouteAuthPolicy, mode: RouteMode) -> Self {
        Self { auth, mode }
    }

    pub const fn protected(mode: RouteMode) -> Self {
        Self::new(RouteAuthPolicy::SessionRequired, mode)
    }

    pub const fn public(mode: RouteMode) -> Self {
        Self::new(RouteAuthPolicy::UnauthenticatedAllowed, mode)
    }
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
    pub policy: RoutePolicy,
}

/// A plugin's published web surface: its routes and (when `api-docs` is on) its
/// OpenAPI fragment and route doc policies. The web subsystem owns this
/// registry, so route publication never touches the core lifecycle registry.
#[derive(Clone, Copy)]
pub struct WebPublication {
    pub name: &'static str,
    pub routes: Option<&'static [RoutePublication]>,
    #[cfg(feature = "api-docs")]
    pub openapi_json: Option<OpenApiJsonFn>,
    #[cfg(feature = "api-docs")]
    pub route_policies: Option<&'static [RouteDocPolicy]>,
}

impl WebPublication {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            routes: None,
            #[cfg(feature = "api-docs")]
            openapi_json: None,
            #[cfg(feature = "api-docs")]
            route_policies: None,
        }
    }

    pub const fn with_routes(mut self, routes: &'static [RoutePublication]) -> Self {
        self.routes = Some(routes);
        self
    }

    #[cfg(feature = "api-docs")]
    pub const fn with_openapi_json(mut self, openapi_json: OpenApiJsonFn) -> Self {
        self.openapi_json = Some(openapi_json);
        self
    }

    #[cfg(feature = "api-docs")]
    pub const fn with_route_doc_policies(
        mut self,
        route_policies: &'static [RouteDocPolicy],
    ) -> Self {
        self.route_policies = Some(route_policies);
        self
    }
}

/// Self-registration handle for a plugin's [`WebPublication`].
pub struct WebPublicationRegistration(pub &'static WebPublication);

inventory::collect!(WebPublicationRegistration);

/// All registered web publications, sorted by name for deterministic route,
/// OpenAPI, and SBOM ordering.
pub fn web_publications() -> Vec<&'static WebPublication> {
    let mut publications: Vec<&'static WebPublication> =
        inventory::iter::<WebPublicationRegistration>()
            .map(|registration| registration.0)
            .collect();
    publications.sort_by_key(|publication| publication.name);
    publications
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
            policies[policy_index] = RouteDocPolicy::new(publication.routes[0].path, publication.policy.auth);
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
