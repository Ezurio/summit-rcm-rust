
//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Shared application-wide D-Bus helpers.

use std::os::fd::AsRawFd;
use anyhow::Result;
use serde_json::{json, Value as JsonValue};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::OnceCell;
use zbus::{
    message::Type as MessageType,
    zvariant::{OwnedValue, Type, Value},
    Connection, MatchRule, MessageStream,
};

pub const DBUS_PROP_IFACE: &str = "org.freedesktop.DBus.Properties";

static SYSTEM_BUS: LazyLock<OnceCell<Connection>> = LazyLock::new(OnceCell::new);

/// Return the shared system bus connection for the application.
pub async fn system_bus() -> Result<&'static Connection> {
    SYSTEM_BUS
        .get_or_try_init(|| async { Connection::system().await.map_err(Into::into) })
        .await
}

pub async fn call_method_deserialize<T, B>(
    conn: &Connection,
    destination: Option<&str>,
    path: &str,
    interface: Option<&str>,
    method: &str,
    body: &B,
) -> Result<T>
where
    T: DeserializeOwned + Type,
    B: Serialize + Type,
{
    let reply = conn
        .call_method(destination, path, interface, method, body)
        .await?;
    reply.body().deserialize().map_err(Into::into)
}

pub async fn get_property<T>(
    conn: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
    property_name: &str,
) -> Result<T>
where
    T: TryFrom<OwnedValue>,
    <T as TryFrom<OwnedValue>>::Error: Into<anyhow::Error>,
{
    let value: OwnedValue = call_method_deserialize(
        conn,
        Some(destination),
        path,
        Some(DBUS_PROP_IFACE),
        "Get",
        &(interface, property_name),
    )
    .await?;
    value.try_into().map_err(Into::into)
}

pub async fn set_property(
    conn: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
    property_name: &str,
    value: Value<'static>,
) -> Result<()> {
    conn.call_method(
        Some(destination),
        path,
        Some(DBUS_PROP_IFACE),
        "Set",
        &(interface, property_name, value),
    )
    .await?;
    Ok(())
}

pub async fn subscribe_to_signal(
    sender: &str,
    interface: &str,
    member: &str,
    max_queued: usize,
) -> Result<MessageStream> {
    let connection = system_bus().await?;
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .sender(sender)
        .and_then(|builder| builder.interface(interface))
        .and_then(|builder| builder.member(member))
        .map(|builder| builder.build())
        .map_err(|_| anyhow::anyhow!("failed to build D-Bus signal match rule"))?;
    MessageStream::for_match_rule(rule, connection, Some(max_queued))
        .await
        .map_err(|error| anyhow::anyhow!("failed to subscribe to D-Bus signal stream: {error}"))
}

pub fn clone_owned_value(value: &OwnedValue) -> Result<OwnedValue> {
    value.try_clone().map_err(Into::into)
}

pub fn try_from_owned_value<T>(value: &OwnedValue) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    clone_owned_value(value)
        .ok()
        .and_then(|value| T::try_from(value).ok())
}

pub fn property<T>(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    properties.get(key).and_then(try_from_owned_value::<T>)
}

#[cfg(feature = "bluetooth")]
pub fn property_or_default<T>(properties: &HashMap<String, OwnedValue>, key: &str) -> T
where
    T: TryFrom<OwnedValue> + Default,
{
    property(properties, key).unwrap_or_default()
}

#[cfg(feature = "bluetooth")]
pub fn property_bytes_hex(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    property::<Vec<u8>>(properties, key).map(hex::encode)
}

fn value_to_json(value: &Value<'_>) -> JsonValue {
    match value {
        Value::U8(v) => json!(v),
        Value::Bool(v) => json!(v),
        Value::I16(v) => json!(v),
        Value::U16(v) => json!(v),
        Value::I32(v) => json!(v),
        Value::U32(v) => json!(v),
        Value::I64(v) => json!(v),
        Value::U64(v) => json!(v),
        Value::F64(v) => json!(v),
        Value::Str(v) => json!(v.to_string()),
        Value::Signature(v) => json!(v.to_string()),
        Value::ObjectPath(v) => json!(v.to_string()),
        Value::Value(v) => value_to_json(v.as_ref()),
        Value::Array(v) => JsonValue::Array(v.inner().iter().map(value_to_json).collect()),
        Value::Dict(v) => JsonValue::Object(
            v.iter()
                .map(|(key, value)| {
                    let key = match value_to_json(key) {
                        JsonValue::String(key) => key,
                        other => other.to_string(),
                    };
                    (key, value_to_json(value))
                })
                .collect(),
        ),
        Value::Structure(v) => JsonValue::Array(v.fields().iter().map(value_to_json).collect()),
        #[cfg(unix)]
        Value::Fd(v) => json!(v.as_raw_fd()),
    }
}

pub fn owned_value_to_json(value: &OwnedValue) -> JsonValue {
    value_to_json(&**value)
}