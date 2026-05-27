//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub use summit_rcm_plugin_api::{PublishedRoute, RouteAuthPolicy};

use axum::Router;

use super::{CapabilityPublication, builtin_registry};

pub type RouteInstaller = fn(Router) -> Router;
type RouteMetadata = &'static [PublishedRoute];

#[derive(Clone, Copy, Debug)]
pub struct RoutePublication {
    pub common: CapabilityPublication<RouteMetadata, RouteInstaller>,
    pub auth: RouteAuthPolicy,
}

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

pub struct HttpPluginPublication {
    pub name: &'static str,
    pub base_api: Option<RouteInstaller>,
    pub routes: Option<&'static [RoutePublication]>,
}

include!(concat!(env!("OUT_DIR"), "/builtin_http_publications.rs"));

impl HttpPluginPublication {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            base_api: None,
            routes: None,
        }
    }

    pub const fn with_base_api_install(mut self, install: RouteInstaller) -> Self {
        self.base_api = Some(install);
        self
    }

    pub const fn with_routes(mut self, routes: &'static [RoutePublication]) -> Self {
        self.routes = Some(routes);
        self
    }
}

pub fn builtin_http_publications() -> Vec<&'static HttpPluginPublication> {
    builtin_registry(BUILTIN_HTTP_PUBLICATIONS)
}

const _: fn(&'static str, &'static str) -> PublishedRoute = PublishedRoute::new;
const _: fn(String, String) -> PublishedRoute = PublishedRoute::leak;
const _: fn(RouteMetadata, RouteInstaller, RouteAuthPolicy) -> RoutePublication = RoutePublication::new;
const _: fn(RouteInstaller, RouteAuthPolicy) -> RoutePublication = RoutePublication::install_only;
const _: fn(HttpPluginPublication, RouteInstaller) -> HttpPluginPublication =
    HttpPluginPublication::with_base_api_install;
const _: fn(HttpPluginPublication, &'static [RoutePublication]) -> HttpPluginPublication =
    HttpPluginPublication::with_routes;
const _: fn(&'static str) -> HttpPluginPublication = HttpPluginPublication::new;