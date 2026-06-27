//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! NetworkManager-owned API route handlers.

pub mod connection_profile;
pub mod shared;

#[cfg(feature = "api-v2")]
pub(crate) mod v2;

#[cfg(feature = "api-legacy")]
pub(crate) mod legacy;
