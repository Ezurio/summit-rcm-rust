//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[derive(Clone, Copy, Debug)]
pub struct CapabilityPublication<Metadata, Installer> {
    #[cfg_attr(not(feature = "at-interface"), allow(dead_code))]
    pub metadata: Option<Metadata>,
    pub install: Installer,
}

impl<Metadata, Installer> CapabilityPublication<Metadata, Installer> {
    #[cfg_attr(not(feature = "at-interface"), allow(dead_code))]
    pub const fn new(metadata: Metadata, install: Installer) -> Self {
        Self {
            metadata: Some(metadata),
            install,
        }
    }

    pub const fn install_only(install: Installer) -> Self {
        Self {
            metadata: None,
            install,
        }
    }
}

mod publication_fields {
    pub mod api {
        #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
        pub type BaseApiField = Option<super::super::super::RouteInstaller>;
        #[cfg(not(any(feature = "api-v2", feature = "api-legacy")))]
        pub type BaseApiField = ();

        #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
        pub type RoutesField = Option<&'static [super::super::super::RoutePublication]>;
        #[cfg(not(any(feature = "api-v2", feature = "api-legacy")))]
        pub type RoutesField = ();

        pub const fn empty_base_api() -> BaseApiField {
            #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
            {
                None
            }
            #[cfg(not(any(feature = "api-v2", feature = "api-legacy")))]
            {
                ()
            }
        }

        pub const fn empty_routes() -> RoutesField {
            #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
            {
                None
            }
            #[cfg(not(any(feature = "api-v2", feature = "api-legacy")))]
            {
                ()
            }
        }
    }

    pub mod at {
        #[cfg(feature = "at-interface")]
        pub type CommandsField = Option<super::super::super::AtCommandPublication>;
        #[cfg(not(feature = "at-interface"))]
        pub type CommandsField = ();

        pub const fn empty_commands() -> CommandsField {
            #[cfg(feature = "at-interface")]
            {
                None
            }
            #[cfg(not(feature = "at-interface"))]
            {
                ()
            }
        }
    }

    pub mod docs {
        #[cfg(feature = "api-docs")]
        pub type OpenApiField = super::super::super::OpenApiPublication;
        #[cfg(not(feature = "api-docs"))]
        pub type OpenApiField = ();

        pub const fn empty_openapi() -> OpenApiField {
            #[cfg(feature = "api-docs")]
            {
                super::super::super::OpenApiPublication::new()
            }
            #[cfg(not(feature = "api-docs"))]
            {
                ()
            }
        }
    }
}

pub struct PluginPublication {
    pub name: &'static str,
    pub base_api: publication_fields::api::BaseApiField,
    pub routes: publication_fields::api::RoutesField,
    #[cfg_attr(not(feature = "at-interface"), allow(dead_code))]
    pub at_commands: publication_fields::at::CommandsField,
    #[cfg_attr(not(feature = "api-docs"), allow(dead_code))]
    pub openapi: publication_fields::docs::OpenApiField,
}

pub struct PluginRegistration {
    pub publication: &'static PluginPublication,
}

inventory::collect!(PluginRegistration);

impl PluginPublication {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            base_api: publication_fields::api::empty_base_api(),
            routes: publication_fields::api::empty_routes(),
            at_commands: publication_fields::at::empty_commands(),
            openapi: publication_fields::docs::empty_openapi(),
        }
    }
}

pub fn builtin_publications() -> Vec<&'static PluginPublication> {
    let mut publications = inventory::iter::<PluginRegistration>
        .into_iter()
        .map(|registration| registration.publication)
        .collect::<Vec<_>>();
    publications.sort_by_key(|publication| publication.name);
    publications
}

const _: fn(&'static str) -> PluginPublication = PluginPublication::new;