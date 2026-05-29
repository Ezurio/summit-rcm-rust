//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
#[path = "publication/core.rs"]
mod core;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
#[path = "publication/http.rs"]
mod http;
#[cfg(feature = "at-interface")]
#[path = "publication/at.rs"]
mod at;
#[cfg(feature = "api-docs")]
#[path = "publication/docs.rs"]
mod docs;
#[path = "publication/macros.rs"]
mod macros;

#[cfg(any(feature = "api-v2", feature = "api-legacy", feature = "at-interface"))]
pub use self::core::*;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub use self::http::*;
#[cfg(feature = "api-docs")]
pub use self::docs::*;