//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub fn builtin_registry<T>(items: &'static [&'static T]) -> Vec<&'static T> {
    items.to_vec()
}

#[derive(Clone, Copy)]
pub struct PluginPublication {
    pub name: &'static str,
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub routes: Option<&'static [super::http::RoutePublication]>,
    #[cfg(feature = "at-interface")]
    pub at_commands: Option<&'static [crate::at_interface::commands::PublishedCommand]>,
    #[cfg(feature = "api-docs")]
    pub openapi_json: Option<super::docs::OpenApiJsonFn>,
    #[cfg(feature = "api-docs")]
    pub route_policies: Option<&'static [super::docs::RouteDocPolicy]>,
    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub startup: Option<fn()>,
}

include!(concat!(env!("OUT_DIR"), "/builtin_plugin_publications.rs"));

impl PluginPublication {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
            routes: None,
            #[cfg(feature = "at-interface")]
            at_commands: None,
            #[cfg(feature = "api-docs")]
            openapi_json: None,
            #[cfg(feature = "api-docs")]
            route_policies: None,
            #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
            startup: None,
        }
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub const fn with_routes(mut self, routes: &'static [super::http::RoutePublication]) -> Self {
        self.routes = Some(routes);
        self
    }

    #[cfg(feature = "at-interface")]
    pub const fn with_at_commands(
        mut self,
        commands: &'static [crate::at_interface::commands::PublishedCommand],
    ) -> Self {
        self.at_commands = Some(commands);
        self
    }

    #[cfg(feature = "api-docs")]
    pub const fn with_openapi_json(mut self, openapi_json: super::docs::OpenApiJsonFn) -> Self {
        self.openapi_json = Some(openapi_json);
        self
    }

    #[cfg(feature = "api-docs")]
    pub const fn with_route_doc_policies(
        mut self,
        route_policies: &'static [super::docs::RouteDocPolicy],
    ) -> Self {
        self.route_policies = Some(route_policies);
        self
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub const fn with_startup(mut self, startup: fn()) -> Self {
        self.startup = Some(startup);
        self
    }
}

pub fn builtin_plugin_publications() -> Vec<&'static PluginPublication> {
    builtin_registry(BUILTIN_PLUGIN_PUBLICATIONS)
}