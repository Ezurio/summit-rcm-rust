//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Seam for plugin-contributed session validation.
//!
//! Core's session-auth middleware lives here, but the session store and its
//! validation logic belong to the login plugin. Core consults a single
//! optional validator registered through [`inventory`]; when none is present
//! (no login plugin built in) there is no way to validate a session, so
//! protected routes are denied.

/// Validates and manages web sessions on behalf of core's auth middleware.
pub trait SessionValidator: Sync {
    /// Returns whether the session is valid, refreshing its expiry if so.
    fn check_and_refresh_session(&self, session_id: i128) -> bool;

    /// Invalidates and removes the session.
    fn remove_session(&self, session_id: i128);
}

/// Self-registration handle for the session validator.
pub struct SessionValidatorRegistration(pub &'static dyn SessionValidator);

inventory::collect!(SessionValidatorRegistration);

/// The registered session validator, if the login plugin is built in.
pub fn session_validator() -> Option<&'static dyn SessionValidator> {
    inventory::iter::<SessionValidatorRegistration>()
        .next()
        .map(|registration| registration.0)
}
