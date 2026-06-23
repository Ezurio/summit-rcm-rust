//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Network route handlers owned by the network plugin.

#[cfg(feature = "api-v2")]
pub mod v2;

#[cfg(feature = "api-legacy")]
pub mod legacy;
