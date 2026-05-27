//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[derive(Clone, Copy, Debug)]
pub struct CapabilityPublication<Metadata, Installer> {
    pub metadata: Option<Metadata>,
    pub install: Installer,
}

impl<Metadata, Installer> CapabilityPublication<Metadata, Installer> {
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

pub fn builtin_registry<T>(items: &'static [&'static T]) -> Vec<&'static T> {
    items.to_vec()
}