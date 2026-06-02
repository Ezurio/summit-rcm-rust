use super::SystemdUnit;
use std::collections::HashMap;
use zbus::zvariant::{OwnedValue, Value as DbusValue};

#[test]
fn active_state_from_properties_prefers_active_state_even_when_load_state_is_not_found() {
    let mut properties = HashMap::<String, OwnedValue>::new();
    properties.insert(
        "LoadState".to_string(),
        OwnedValue::try_from(DbusValue::from("not-found")).expect("owned dbus value"),
    );
    properties.insert(
        "ActiveState".to_string(),
        OwnedValue::try_from(DbusValue::from("inactive")).expect("owned dbus value"),
    );

    assert_eq!(
        SystemdUnit::active_state_from_properties(&properties),
        "inactive"
    );
}

#[test]
fn active_state_from_properties_returns_empty_string_when_active_state_missing() {
    let properties = HashMap::<String, OwnedValue>::new();

    assert_eq!(SystemdUnit::active_state_from_properties(&properties), "");
}
