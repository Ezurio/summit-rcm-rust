//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! In-process provisioning state machine.
//!
//! All writes to the provisioning state file go through this module. Route
//! handlers emit events; the state machine validates the transition,
//! persists the new state, and requests the post-transition daemon restart.
//!
//! See `docs/provisioning-flow.md` for the canonical flow.

use crate::service::{CertificateProvisioningService, ProvisioningState};
use std::fmt;

/// Events that drive the provisioning state machine.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Event {
    /// A device certificate was successfully verified and installed.
    /// Valid only in `Unprovisioned`; transitions to `PartiallyProvisioned`.
    CertUploaded,

    /// A paired client bundle was successfully installed.
    /// Valid only in `PartiallyProvisioned`; no state transition, but the
    /// daemon is restarted so the new trust store takes effect.
    #[cfg(feature = "api-v2")]
    ClientBundleUploaded,

    /// A manual time set succeeded. Transitions `PartiallyProvisioned` to
    /// `FullyProvisioned`. In any other state this is a no-op.
    ManualTimeSet,
}

/// Errors returned by the state machine.
#[derive(Debug)]
pub(crate) enum TransitionError {
    /// The event is not valid in the current state.
    WrongState {
        current: ProvisioningState,
        event: Event,
    },
    /// Writing the state file failed.
    Persist(anyhow::Error),
    /// Requesting the daemon restart failed.
    Restart(anyhow::Error),
}

/// Outcome of a successful `handle` call.
pub(crate) struct ProvisioningStateMachine;

impl ProvisioningStateMachine {
    pub(crate) async fn current() -> ProvisioningState {
        CertificateProvisioningService::get_provisioning_state_async().await
    }

    pub(crate) async fn handle(event: Event) -> Result<(), TransitionError> {
        let current = Self::current().await;
        match event {
            Event::CertUploaded => {
                if current != ProvisioningState::Unprovisioned {
                    return Err(TransitionError::WrongState { current, event });
                }
                Self::transition_to(current, ProvisioningState::PartiallyProvisioned).await
            }
            #[cfg(feature = "api-v2")]
            Event::ClientBundleUploaded => {
                if current != ProvisioningState::PartiallyProvisioned {
                    return Err(TransitionError::WrongState { current, event });
                }
                CertificateProvisioningService::restart_summit_rcm()
                    .await
                    .map_err(TransitionError::Restart)?;
                Ok(())
            }
            Event::ManualTimeSet => {
                if current == ProvisioningState::PartiallyProvisioned {
                    Self::transition_to(current, ProvisioningState::FullyProvisioned).await
                } else {
                    Ok(())
                }
            }
        }
    }

    async fn transition_to(
        previous: ProvisioningState,
        next: ProvisioningState,
    ) -> Result<(), TransitionError> {
        CertificateProvisioningService::set_provisioning_state_async(next)
            .await
            .map_err(TransitionError::Persist)?;
        CertificateProvisioningService::restart_summit_rcm()
            .await
            .map_err(TransitionError::Restart)?;
        let _ = previous;
        let _ = next;
        Ok(())
    }
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongState { current, event } => {
                write!(f, "invalid provisioning transition: event {:?} in state {:?}", event, current)
            }
            Self::Persist(error) => write!(f, "failed to persist provisioning state: {error}"),
            Self::Restart(error) => write!(f, "failed to restart summit-rcm: {error}"),
        }
    }
}

impl std::error::Error for TransitionError {}
