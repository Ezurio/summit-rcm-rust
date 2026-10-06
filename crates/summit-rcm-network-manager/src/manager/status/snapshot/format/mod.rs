//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Formatting helpers that convert raw NetworkManager status snapshots into the
//! Summit RCM (v2) and legacy JSON shapes.
//!
//! Shared, shape-agnostic extraction lives in [`shared`]; the v2 and legacy
//! responses are produced by independent encoders in [`v2`] and [`legacy`]. The
//! v2 (modern, camelCase) encoder is also used by the AT command interface, so
//! it is available whenever either `api-v2` or `at-interface` is enabled.

#[cfg(feature = "api-legacy")]
mod legacy;
mod shared;
#[cfg(any(feature = "api-v2", feature = "at-interface"))]
mod v2;
