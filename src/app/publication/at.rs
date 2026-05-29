//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use super::CapabilityPublication;

pub type AtCommandPublication = CapabilityPublication<(), &'static [crate::at_interface::commands::PublishedCommand]>;