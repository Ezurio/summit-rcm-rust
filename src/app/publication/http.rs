//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use axum::Router;

use super::{CapabilityPublication, PluginPublication};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedRoute {
    pub method: &'static str,
    pub path: &'static str,
}

impl PublishedRoute {
    pub const fn new(method: &'static str, path: &'static str) -> Self {
        Self {
            method,
            path,
        }
    }

    pub fn leak(method: String, path: String) -> Self {
        Self {
            method: Box::leak(method.into_boxed_str()),
            path: Box::leak(path.into_boxed_str()),
        }
    }
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub type RouteInstaller = fn(Router) -> Router;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub type RouteMetadata = &'static [PublishedRoute];

#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "api-docs"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteAuthPolicy {
    SessionRequired,
    UnauthenticatedAllowed,
}

#[cfg(feature = "api-docs")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteDocPolicy {
    pub path: &'static str,
    pub auth: RouteAuthPolicy,
}

#[cfg(feature = "api-docs")]
impl RouteDocPolicy {
    pub const fn new(path: &'static str, auth: RouteAuthPolicy) -> Self {
        Self { path, auth }
    }
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
#[derive(Clone, Copy, Debug)]
pub struct RoutePublication {
    pub common: CapabilityPublication<RouteMetadata, RouteInstaller>,
    pub auth: RouteAuthPolicy,
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
impl RoutePublication {
    pub const fn new(routes: RouteMetadata, install: RouteInstaller, auth: RouteAuthPolicy) -> Self {
        Self {
            common: CapabilityPublication::new(routes, install),
            auth,
        }
    }

    pub const fn install_only(install: RouteInstaller, auth: RouteAuthPolicy) -> Self {
        Self {
            common: CapabilityPublication::install_only(install),
            auth,
        }
    }
}

#[cfg(feature = "api-docs")]
pub type OpenApiDocFn = fn() -> utoipa::openapi::OpenApi;

#[cfg(feature = "api-docs")]
pub struct OpenApiPublication {
    pub docs: [Option<OpenApiDocFn>; 2],
    pub route_policies: Option<&'static [RouteDocPolicy]>,
}

#[cfg(feature = "api-docs")]
impl OpenApiPublication {
    pub const fn new() -> Self {
        Self {
            docs: [None, None],
            route_policies: None,
        }
    }
}

impl PluginPublication {
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub const fn with_base_api_install(mut self, install: RouteInstaller) -> Self {
        self.base_api = Some(install);
        self
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub const fn with_routes(mut self, routes: &'static [RoutePublication]) -> Self {
        self.routes = Some(routes);
        self
    }

    #[cfg(feature = "api-docs")]
    pub const fn with_openapi_docs(mut self, docs: &'static [OpenApiDocFn]) -> Self {
        if !docs.is_empty() {
            self.openapi.docs[0] = Some(docs[0]);
        }
        if docs.len() > 1 {
            self.openapi.docs[1] = Some(docs[1]);
        }
        self
    }

    #[cfg(feature = "api-docs")]
    pub const fn with_route_doc_policies(mut self, route_policies: &'static [RouteDocPolicy]) -> Self {
        self.openapi.route_policies = Some(route_policies);
        self
    }
}

const _: fn(&'static str, &'static str) -> PublishedRoute = PublishedRoute::new;
const _: fn(String, String) -> PublishedRoute = PublishedRoute::leak;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const _: fn(RouteMetadata, RouteInstaller, RouteAuthPolicy) -> RoutePublication = RoutePublication::new;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const _: fn(RouteInstaller, RouteAuthPolicy) -> RoutePublication = RoutePublication::install_only;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const _: fn(PluginPublication, RouteInstaller) -> PluginPublication =
    PluginPublication::with_base_api_install;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const _: fn(PluginPublication, &'static [RoutePublication]) -> PluginPublication =
    PluginPublication::with_routes;
#[cfg(feature = "api-docs")]
const _: fn(PluginPublication, &'static [OpenApiDocFn]) -> PluginPublication =
    PluginPublication::with_openapi_docs;
#[cfg(feature = "api-docs")]
const _: fn(PluginPublication, &'static [RouteDocPolicy]) -> PluginPublication =
    PluginPublication::with_route_doc_policies;