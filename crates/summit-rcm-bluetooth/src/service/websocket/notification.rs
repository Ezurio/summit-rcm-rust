//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! Typed BLE notification frames and their serialization.
//!
//! Each frame is a plain `#[derive(Serialize)]` struct whose field declaration
//! order is the wire order, so there is no manual JSON construction. Frames are
//! rendered as compact JSON followed by a trailing newline. Topics that the
//! Python producer serialized with `sort_keys=True` (`discovery`, `connect`)
//! declare their fields in alphabetical order; the `char` topic keeps insertion
//! order. Nested service maps use [`BTreeMap`], which is inherently sorted.

use serde::Serialize;
use std::collections::BTreeMap;
use summit_rcm_web::serde_json;
use summit_rcm_web::serde_json::Value;

/// One BLE notification frame. Serialized as an externally tagged object so the
/// variant name becomes the topic key (`{"discovery": {…}}`, `{"connect": {…}}`,
/// `{"char": {…}}`).
#[derive(Serialize)]
pub enum BleNotification {
    #[serde(rename = "discovery")]
    Discovery(DiscoveryNotification),
    #[serde(rename = "connect")]
    Connect(ConnectNotification),
    #[serde(rename = "char")]
    CharValue(CharValueNotification),
    #[serde(rename = "char")]
    CharResult(CharResultNotification),
}

impl BleNotification {
    /// Render the topic-wrapped frame as compact JSON with a trailing newline.
    pub fn render(&self) -> String {
        let mut payload =
            serde_json::to_string(self).expect("BLE notification serialization is infallible");
        payload.push('\n');
        payload
    }

    /// Render and publish the frame to the shared notification bus.
    pub(crate) fn publish(self) {
        summit_rcm_web::notifications::publish(self.render());
    }
}

/// A newly discovered device, published on the `discovery` topic.
///
/// `properties` holds the curated BlueZ discovery fields under their original
/// keys (`Address`, `Name`, …), so no per-field renaming is needed. It is
/// flattened next to `timestamp` and, because serialization sorts keys, the
/// frame matches Python's `sort_keys=True` ordering.
#[derive(Serialize)]
pub struct DiscoveryNotification {
    #[serde(flatten)]
    pub properties: BTreeMap<String, Value>,
    pub timestamp: u64,
}

/// A device connection-state change, published on the `connect` topic. The
/// disconnect case reuses this frame with `connected = false` and no services.
/// Fields are declared in alphabetical order to match Python's `sort_keys`.
#[derive(Serialize)]
pub struct ConnectNotification {
    pub address: String,
    pub alias: String,
    pub connected: bool,
    pub name: String,
    pub paired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services: Option<BTreeMap<String, ServicesEntry>>,
    pub timestamp: u64,
}

impl ConnectNotification {
    /// Build a disconnect frame for a device that vanished from the bus: only
    /// the address is known, so the remaining fields carry their
    /// disconnected/empty defaults.
    pub fn disconnected(address: String, timestamp: u64) -> Self {
        Self {
            address,
            alias: String::new(),
            connected: false,
            name: String::new(),
            paired: false,
            services: None,
            timestamp,
        }
    }
}

/// GATT service entry within a `connect` frame: a list of single-key
/// characteristic maps, mirroring the Python `{characteristics: [{uuid: {…}}]}`
/// shape.
#[derive(Serialize)]
pub struct ServicesEntry {
    pub characteristics: Vec<BTreeMap<String, CharacteristicFlags>>,
}

/// Characteristic descriptor carrying its BlueZ access flags.
#[derive(Serialize)]
pub struct CharacteristicFlags {
    #[serde(rename = "Flags")]
    pub flags: Vec<String>,
}

/// A characteristic value notification, published on the `char` topic with
/// insertion (non-sorted) field order.
#[derive(Serialize)]
pub struct CharValueNotification {
    pub char_uuid: String,
    pub value: String,
    pub timestamp: u64,
}

/// A characteristic operation result, published on the `char` topic with
/// insertion (non-sorted) field order. `error` is always present, rendering as
/// `null` on success.
#[derive(Serialize)]
pub struct CharResultNotification {
    pub char_uuid: String,
    pub result: i32,
    pub error: Option<String>,
    pub timestamp: u64,
}
