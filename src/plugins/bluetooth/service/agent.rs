//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! BlueZ pairing agent (`org.bluez.Agent1`).
//!
//! Mirrors the Python `AuthenticationAgent`/`AgentSingleton`: it exports a
//! `NoInputNoOutput` agent at [`AGENT_PATH`], registers it with
//! `org.bluez.AgentManager1`, and answers the pairing callbacks BlueZ invokes
//! during pairing. Registration is best-effort and idempotent — a host without
//! a live BlueZ (e.g. the unit-test bus) simply logs and continues, matching the
//! Python behaviour where agent registration failures are swallowed.

use super::*;
use tokio::sync::Mutex as AsyncMutex;
use zbus::zvariant::OwnedObjectPath;

const AGENT_PATH: &str = "/com/summit/agent";
const AGENT_MANAGER_IFACE: &str = "org.bluez.AgentManager1";
const AGENT_CAPABILITY: &str = "NoInputNoOutput";
const BLUEZ_ROOT_PATH: &str = "/org/bluez";

/// Passkeys preset by clients, keyed by device object path, returned from
/// [`AuthenticationAgent::request_passkey`] during pairing.
static AGENT_PASSKEYS: LazyLock<Mutex<HashMap<String, u32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Guards one-time agent export + registration.
static AGENT_REGISTERED: AsyncMutex<bool> = AsyncMutex::const_new(false);

/// Store a passkey for a device path so the agent can return it when BlueZ asks.
pub(super) fn set_passkey(device_path: &str, passkey: u32) {
    AGENT_PASSKEYS
        .lock()
        .expect("agent passkey mutex poisoned")
        .insert(device_path.to_string(), passkey);
}

/// Export and register the pairing agent once on the supplied connection.
/// Best-effort: export or registration failures (such as on a bus without
/// `AgentManager1`, e.g. the unit-test bus) are logged and ignored, leaving the
/// agent unregistered so a later call can retry. Using the caller's connection
/// keeps the agent on the same bus as the rest of the command.
pub(super) async fn ensure_agent_registered(conn: &Connection) {
    let mut registered = AGENT_REGISTERED.lock().await;
    if *registered {
        return;
    }

    // Exporting is idempotent; `Ok(false)` means it was already present.
    if let Err(error) = conn.object_server().at(AGENT_PATH, AuthenticationAgent).await {
        log::debug!("bluetooth agent: export failed: {error}");
        return;
    }

    let agent_path = match OwnedObjectPath::try_from(AGENT_PATH) {
        Ok(path) => path,
        Err(error) => {
            log::debug!("bluetooth agent: invalid agent path: {error}");
            return;
        }
    };

    match dbus::call_method(
        conn,
        Some(BLUEZ_SERVICE),
        BLUEZ_ROOT_PATH,
        Some(AGENT_MANAGER_IFACE),
        "RegisterAgent",
        &(agent_path, AGENT_CAPABILITY),
        None,
    )
    .await
    {
        Ok(_) => *registered = true,
        Err(error) => log::debug!("bluetooth agent: RegisterAgent failed: {error}"),
    }
}

/// Mark a device trusted so subsequent connections do not re-prompt, matching
/// the Python `set_trusted` helper invoked from every agent callback.
async fn set_trusted(device_path: &str) {
    let Ok(conn) = dbus::system_bus().await else {
        return;
    };
    if let Err(error) = dbus::set_property(
        conn.as_ref(),
        BLUEZ_SERVICE,
        device_path,
        DEVICE_IFACE,
        "Trusted",
        Value::from(true),
    )
    .await
    {
        log::debug!("bluetooth agent: set Trusted failed for {device_path}: {error}");
    }
}

/// `org.bluez.Agent1` implementation for a `NoInputNoOutput` agent.
struct AuthenticationAgent;

#[zbus::interface(name = "org.bluez.Agent1")]
impl AuthenticationAgent {
    fn release(&self) {
        log::debug!("bluetooth agent: Release");
    }

    fn authorize_service(&self, device: OwnedObjectPath, uuid: String) {
        log::debug!("bluetooth agent: AuthorizeService ({device}, {uuid})");
    }

    async fn request_pin_code(&self, device: OwnedObjectPath) -> String {
        log::debug!("bluetooth agent: RequestPinCode ({device})");
        set_trusted(device.as_str()).await;
        "000000".to_string()
    }

    fn display_pin_code(&self, device: OwnedObjectPath, pincode: String) {
        log::debug!("bluetooth agent: DisplayPinCode ({device}, {pincode})");
    }

    async fn request_passkey(&self, device: OwnedObjectPath) -> u32 {
        log::debug!("bluetooth agent: RequestPasskey ({device})");
        set_trusted(device.as_str()).await;
        AGENT_PASSKEYS
            .lock()
            .expect("agent passkey mutex poisoned")
            .get(device.as_str())
            .copied()
            .unwrap_or(0)
    }

    fn display_passkey(&self, device: OwnedObjectPath, passkey: u32, entered: u16) {
        log::debug!("bluetooth agent: DisplayPasskey ({device}, {passkey:06}, entered {entered})");
    }

    async fn request_confirmation(&self, device: OwnedObjectPath, passkey: u32) {
        log::debug!("bluetooth agent: RequestConfirmation ({device}, {passkey:06})");
        set_trusted(device.as_str()).await;
    }

    fn request_authorization(&self, device: OwnedObjectPath) {
        log::debug!("bluetooth agent: RequestAuthorization ({device})");
    }

    fn cancel(&self) {
        log::debug!("bluetooth agent: Cancel");
    }
}
