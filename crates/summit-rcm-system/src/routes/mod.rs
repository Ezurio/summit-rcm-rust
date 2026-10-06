//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[cfg(feature = "api-legacy")]
pub(crate) mod legacy;
pub(crate) mod shared;
#[cfg(feature = "api-v2")]
pub(crate) mod v2;
