//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! BLE notification websocket producer.
//!
//! A self-contained, event-driven broadcaster compiled only with the
//! `bluetooth-websocket` feature. It subscribes to the BlueZ object-manager and
//! property-change signals and publishes typed BLE notification frames onto the
//! shared application notification bus (`crate::notifications`). There is no
//! polling loop and no snapshot/diff engine: each frame is built directly from
//! the signal that triggered it, mirroring the Python source-of-truth.
//!
//! The producer holds no compile-time knowledge of device plugins (HID, VSP,
//! …); those plugins subscribe to the BlueZ signals they care about on their
//! own. Adding a new device client therefore never touches this module.

use super::*;
use std::sync::Mutex;
use tokio::task::JoinHandle;

pub(crate) mod notification;
mod observers;

/// Signals retained per observer before a slow consumer lags.
const SIGNAL_BUFFER: usize = 64;

struct ProducerState {
    enabled: bool,
    tasks: Vec<JoinHandle<()>>,
}

static PRODUCER_STATE: Mutex<ProducerState> = Mutex::new(ProducerState {
    enabled: false,
    tasks: Vec::new(),
});

impl BluetoothService {
    pub fn websocket_notifications_enabled() -> bool {
        PRODUCER_STATE.lock().unwrap().enabled
    }
}

/// Enable websocket notifications: mark the producer enabled (so the upgrade
/// routes accept clients) and start the BlueZ signal observers. A failure to
/// subscribe is logged but not fatal, so the REST command still succeeds on a
/// host without a live BlueZ.
pub(super) async fn enable_notifications() -> anyhow::Result<()> {
    PRODUCER_STATE.lock().unwrap().enabled = true;
    ensure_observers().await;
    Ok(())
}

async fn ensure_observers() {
    {
        let state = PRODUCER_STATE.lock().unwrap();
        if state.tasks.iter().any(|task| !task.is_finished()) {
            return;
        }
    }

    let mut observers = dbus::SignalObservers::new(BLUEZ_SERVICE, SIGNAL_BUFFER);
    observers
        .add(OBJECT_MANAGER_IFACE, "InterfacesAdded", |message| async move {
            observers::on_interfaces_added(message).await
        })
        .await;
    observers
        .add(OBJECT_MANAGER_IFACE, "InterfacesRemoved", |message| async move {
            observers::on_interfaces_removed(message).await
        })
        .await;
    observers
        .add(dbus::DBUS_PROP_IFACE, "PropertiesChanged", |message| async move {
            observers::on_properties_changed(message).await
        })
        .await;
    let tasks = observers.into_tasks();

    let mut state = PRODUCER_STATE.lock().unwrap();
    if state.tasks.iter().any(|task| !task.is_finished()) {
        for task in tasks {
            task.abort();
        }
    } else {
        for task in state.tasks.drain(..) {
            task.abort();
        }
        state.tasks = tasks;
    }
}
