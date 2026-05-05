//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[path = "publication/core.rs"]
mod core;
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "api-docs"))]
#[path = "publication/http.rs"]
mod http;
#[cfg(feature = "at-interface")]
#[path = "publication/at.rs"]
mod at;
#[path = "publication/macros.rs"]
mod macros;

pub use self::core::*;
#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "api-docs"))]
pub use self::http::*;
#[cfg(feature = "at-interface")]
pub use self::at::*;