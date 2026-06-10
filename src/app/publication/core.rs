//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[derive(Clone, Copy)]
pub struct PluginPublication {
    pub name: &'static str,
    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub routes: Option<&'static [super::http::RoutePublication]>,
    #[cfg(feature = "at-interface")]
    pub at_commands: Option<&'static [crate::at_interface::commands::PublishedCommand]>,
    #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
    pub openapi_json: Option<super::http::OpenApiJsonFn>,
    #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
    pub route_policies: Option<&'static [super::http::RouteDocPolicy]>,
    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub startup: Option<fn()>,
    /// Bluetooth device command handler contributed by a device plugin (HID,
    /// VSP, …). The core bluetooth plugin collects these across all
    /// publications, so a new device plugin participates in the shared command
    /// pipeline without any edit to the core bluetooth code.
    #[cfg(feature = "bluetooth")]
    pub(crate) bluetooth_command_handler:
        Option<&'static dyn crate::plugins::bluetooth::service::BluetoothCommandHandler>,
}

/// Self-registration handle for a builtin plugin's [`PluginPublication`].
///
/// Each plugin's `declare_plugin!` invocation submits one of these via
/// [`inventory`], so the set of builtin publications is assembled from the
/// plugins that are actually compiled in — no central registry to maintain.
pub struct PluginPublicationRegistration(pub &'static PluginPublication);

inventory::collect!(PluginPublicationRegistration);

impl PluginPublication {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
            routes: None,
            #[cfg(feature = "at-interface")]
            at_commands: None,
            #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
            openapi_json: None,
            #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
            route_policies: None,
            #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
            startup: None,
            #[cfg(feature = "bluetooth")]
            bluetooth_command_handler: None,
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

    #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
    pub const fn with_openapi_json(mut self, openapi_json: super::http::OpenApiJsonFn) -> Self {
        self.openapi_json = Some(openapi_json);
        self
    }

    #[cfg(all(feature = "api-docs", any(feature = "api-v2", feature = "api-legacy")))]
    pub const fn with_route_doc_policies(
        mut self,
        route_policies: &'static [super::http::RouteDocPolicy],
    ) -> Self {
        self.route_policies = Some(route_policies);
        self
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
    pub const fn with_startup(mut self, startup: fn()) -> Self {
        self.startup = Some(startup);
        self
    }

    #[cfg(feature = "bluetooth")]
    pub const fn with_bluetooth_command_handler(
        mut self,
        handler: &'static dyn crate::plugins::bluetooth::service::BluetoothCommandHandler,
    ) -> Self {
        self.bluetooth_command_handler = Some(handler);
        self
    }
}

pub fn builtin_plugin_publications() -> Vec<&'static PluginPublication> {
    let mut publications: Vec<&'static PluginPublication> =
        inventory::iter::<PluginPublicationRegistration>()
            .map(|registration| registration.0)
            .collect();
    // `inventory` yields registrations in link order, which is not stable
    // across builds; sort by name so route ordering, OpenAPI, and SBOM output
    // stay deterministic.
    publications.sort_by_key(|publication| publication.name);
    publications
}