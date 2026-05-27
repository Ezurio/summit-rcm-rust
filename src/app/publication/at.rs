//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

pub use summit_rcm_plugin_api::PublishedAtCommand;

use super::{CapabilityPublication, builtin_registry};

pub type AtCommandInstaller =
    fn(&mut Vec<Box<dyn crate::at_interface::commands::Command>>);
pub type AtCommandMetadata = &'static [PublishedAtCommand];
pub type AtCommandPublication = CapabilityPublication<AtCommandMetadata, AtCommandInstaller>;

pub struct AtPluginPublication {
    pub name: &'static str,
    pub at_commands: Option<AtCommandPublication>,
}

include!(concat!(env!("OUT_DIR"), "/builtin_at_publications.rs"));

impl AtPluginPublication {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            at_commands: None,
        }
    }

    pub const fn with_at_command_routes(
        mut self,
        commands: AtCommandMetadata,
        install: AtCommandInstaller,
    ) -> Self {
        self.at_commands = Some(AtCommandPublication::new(commands, install));
        self
    }
}

pub fn builtin_at_publications() -> Vec<&'static AtPluginPublication> {
    builtin_registry(BUILTIN_AT_PUBLICATIONS)
}

const _: fn(&'static str, &'static str) -> PublishedAtCommand = PublishedAtCommand::new;
const _: fn(AtCommandMetadata, AtCommandInstaller) -> AtCommandPublication = |commands, install| {
    AtCommandPublication::new(commands, install)
};
const _: fn(AtCommandInstaller) -> AtCommandPublication = AtCommandPublication::install_only;
const _: fn(AtPluginPublication, AtCommandMetadata, AtCommandInstaller) -> AtPluginPublication =
    AtPluginPublication::with_at_command_routes;
const _: fn(&'static str) -> AtPluginPublication = AtPluginPublication::new;