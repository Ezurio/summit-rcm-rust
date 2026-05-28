//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::CapabilityPublication;

pub type AtCommandPublication = CapabilityPublication<(), &'static [crate::at_interface::commands::PublishedCommand]>;

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

    pub const fn with_at_commands(
        mut self,
        commands: &'static [crate::at_interface::commands::PublishedCommand],
    ) -> Self {
        self.at_commands = Some(AtCommandPublication::install_only(commands));
        self
    }
}

pub const fn builtin_at_publications_slice() -> &'static [&'static AtPluginPublication] {
    BUILTIN_AT_PUBLICATIONS
}