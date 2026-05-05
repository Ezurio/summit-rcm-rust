//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::{CapabilityPublication, PluginPublication};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedAtCommand {
    pub signature: &'static str,
    pub name: &'static str,
}

impl PublishedAtCommand {
    pub const fn new(signature: &'static str, name: &'static str) -> Self {
        Self { signature, name }
    }
}

pub type AtCommandInstaller =
    fn(&mut Vec<Box<dyn crate::at_interface::commands::Command>>);
pub type AtCommandMetadata = &'static [PublishedAtCommand];
pub type AtCommandPublication = CapabilityPublication<AtCommandMetadata, AtCommandInstaller>;

impl PluginPublication {
    pub const fn with_at_command_routes(
        mut self,
        commands: AtCommandMetadata,
        install: AtCommandInstaller,
    ) -> Self {
        self.at_commands = Some(AtCommandPublication::new(commands, install));
        self
    }
}

const _: fn(&'static str, &'static str) -> PublishedAtCommand = PublishedAtCommand::new;
const _: fn(AtCommandMetadata, AtCommandInstaller) -> AtCommandPublication = |commands, install| {
    AtCommandPublication::new(commands, install)
};
const _: fn(AtCommandInstaller) -> AtCommandPublication = AtCommandPublication::install_only;
const _: fn(PluginPublication, AtCommandMetadata, AtCommandInstaller) -> PluginPublication =
    PluginPublication::with_at_command_routes;