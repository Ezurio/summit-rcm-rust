//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! In-process provisioning state machine.
//!
//! All writes to the provisioning state file go through this module. Route
//! handlers emit events; the state machine validates the transition,
//! persists the new state, and schedules the post-transition daemon restart.
//!
//! See `docs/provisioning-flow.md` for the canonical flow.

use std::time::Duration;

use log::error;

use crate::plugins::provisioning::service::{CertificateProvisioningService, ProvisioningState};

/// Events that drive the provisioning state machine.
#[derive(Debug, Clone, Copy)]
pub enum Event {
    /// A device certificate was successfully verified and installed.
    /// Valid only in `Unprovisioned`; transitions to `PartiallyProvisioned`.
    CertUploaded,

    /// A paired client bundle was successfully installed.
    /// Valid only in `PartiallyProvisioned`; no state transition, but the
    /// daemon is restarted so the new trust store takes effect.
    ClientBundleUploaded,

    /// A manual time set succeeded. Transitions `PartiallyProvisioned` to
    /// `FullyProvisioned`. In any other state this is a no-op.
    ManualTimeSet,
}

/// Errors returned by the state machine.
#[derive(Debug)]
pub enum TransitionError {
    /// The event is not valid in the current state.
    WrongState {
        current: ProvisioningState,
        event: Event,
    },
    /// Writing the state file failed.
    Persist(anyhow::Error),
}

/// Outcome of a successful `handle` call.
#[derive(Debug, Clone, Copy)]
pub struct TransitionOutcome {
    pub previous: ProvisioningState,
    pub current: ProvisioningState,
}

const RESTART_DELAY: Duration = Duration::from_millis(100);

pub struct ProvisioningStateMachine;

impl ProvisioningStateMachine {
    pub async fn current() -> ProvisioningState {
        CertificateProvisioningService::get_provisioning_state_async().await
    }

    pub async fn handle(event: Event) -> Result<TransitionOutcome, TransitionError> {
        let current = Self::current().await;
        match event {
            Event::CertUploaded => {
                if current != ProvisioningState::Unprovisioned {
                    return Err(TransitionError::WrongState { current, event });
                }
                Self::transition_to(current, ProvisioningState::PartiallyProvisioned).await
            }
            Event::ClientBundleUploaded => {
                if current != ProvisioningState::PartiallyProvisioned {
                    return Err(TransitionError::WrongState { current, event });
                }
                Self::schedule_restart();
                Ok(TransitionOutcome {
                    previous: current,
                    current,
                })
            }
            Event::ManualTimeSet => {
                if current == ProvisioningState::PartiallyProvisioned {
                    Self::transition_to(current, ProvisioningState::FullyProvisioned).await
                } else {
                    Ok(TransitionOutcome {
                        previous: current,
                        current,
                    })
                }
            }
        }
    }

    async fn transition_to(
        previous: ProvisioningState,
        next: ProvisioningState,
    ) -> Result<TransitionOutcome, TransitionError> {
        CertificateProvisioningService::set_provisioning_state_async(next)
            .await
            .map_err(TransitionError::Persist)?;
        Self::schedule_restart();
        Ok(TransitionOutcome {
            previous,
            current: next,
        })
    }

    fn schedule_restart() {
        tokio::spawn(async {
            tokio::time::sleep(RESTART_DELAY).await;
            if !CertificateProvisioningService::restart_summit_rcm().await {
                error!("Provisioning state-machine restart of summit-rcm failed");
            }
        });
    }
}
