#!/usr/bin/env python3

from __future__ import annotations

import argparse
import re
import textwrap
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = REPO_ROOT / "src/plugins/network_manager/routes/connection_profile.rs"

TARGET_SETTINGS = [
    "connection",
    "802-1x",
    "gsm",
    "ipv4",
    "ipv6",
    "802-3-ethernet",
    "802-11-wireless",
    "802-11-wireless-security",
    "wifi-p2p",
]

STRUCT_NAMES = {
    "connection": "ConnectionSettingsConnection",
    "802-1x": "ConnectionSettings8021x",
    "gsm": "ConnectionSettingsGsm",
    "ipv4": "ConnectionSettingsIp4Config",
    "ipv6": "ConnectionSettingsIp6Config",
    "802-3-ethernet": "ConnectionSettingsWired",
    "802-11-wireless": "ConnectionSettingsWireless",
    "802-11-wireless-security": "ConnectionSettingsWirelessSecurity",
    "wifi-p2p": "ConnectionSettingsWifiP2p",
}

TOP_LEVEL_FIELDS = [
    ("802-1x", "enterprise_auth", True),
    ("connection", "connection", False),
    ("gsm", "gsm", True),
    ("ipv4", "ipv4", True),
    ("ipv6", "ipv6", True),
    ("802-3-ethernet", "wired", True),
    ("802-11-wireless", "wireless", True),
    ("802-11-wireless-security", "wireless_security", True),
    ("wifi-p2p", "wifi_p2p", True),
]

RUST_KEYWORDS = {
    "as",
    "break",
    "const",
    "continue",
    "crate",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
    "async",
    "await",
    "dyn",
}

FIELD_NAME_OVERRIDES = {
    ("connection", "type"): "connection_type",
    ("802-1x", "optional"): "is_optional",
}

TYPE_OVERRIDES = {
    ("802-11-wireless", "security"): "Option<Value>",
    ("802-3-ethernet", "s390-options"): "Option<BTreeMap<String, String>>",
    ("ipv4", "address-data"): "Option<Vec<BTreeMap<String, Value>>>",
    ("ipv4", "route-data"): "Option<Vec<BTreeMap<String, Value>>>",
    ("ipv4", "routing-rules"): "Option<Vec<BTreeMap<String, Value>>>",
    ("ipv6", "address-data"): "Option<Vec<BTreeMap<String, Value>>>",
    ("ipv6", "route-data"): "Option<Vec<BTreeMap<String, Value>>>",
    ("ipv6", "routing-rules"): "Option<Vec<BTreeMap<String, Value>>>",
}

STRING_OR_ARRAY_FIELDS = {
    ("connection", "ip-ping-addresses"),
    ("connection", "permissions"),
    ("connection", "secondaries"),
    ("802-1x", "altsubject-matches"),
    ("802-1x", "eap"),
    ("802-1x", "phase2-altsubject-matches"),
    ("802-1x", "phase2-auth"),
    ("802-1x", "phase2-autheap"),
    ("ipv4", "dhcp-reject-servers"),
    ("ipv4", "dns-data"),
    ("ipv4", "dns-options"),
    ("ipv4", "dns-search"),
    ("ipv6", "dhcp-reject-servers"),
    ("ipv6", "dns"),
    ("ipv6", "dns-data"),
    ("ipv6", "dns-options"),
    ("ipv6", "dns-search"),
    ("802-3-ethernet", "mac-address-blacklist"),
    ("802-3-ethernet", "mac-address-denylist"),
    ("802-3-ethernet", "s390-subchannels"),
    ("802-11-wireless", "mac-address-blacklist"),
    ("802-11-wireless", "mac-address-denylist"),
    ("802-11-wireless", "seen-bssids"),
    ("802-11-wireless-security", "group"),
    ("802-11-wireless-security", "pairwise"),
    ("802-11-wireless-security", "proto"),
}

REQUIRED_FIELDS = {
    ("802-11-wireless", "ssid"),
    ("802-11-wireless-security", "key-mgmt"),
}

DBUS_TYPE_MAP = {
    "b": "Option<bool>",
    "s": "Option<String>",
    "i": "Option<i64>",
    "u": "Option<i64>",
    "t": "Option<i64>",
    "x": "Option<i64>",
    "y": "Option<i64>",
    "as": "Option<Vec<String>>",
    "au": "Option<Vec<i64>>",
    "ay": "Option<String>",
    "aay": "Option<Vec<String>>",
    "aau": "Option<Vec<String>>",
    "a(ayuay)": "Option<Vec<String>>",
    "a(ayuayu)": "Option<Vec<String>>",
    "a{ss}": "Option<BTreeMap<String, String>>",
    "a{sv}": "Option<BTreeMap<String, Value>>",
    "aa{sv}": "Option<Vec<BTreeMap<String, Value>>>",
}


@dataclass
class PropertyInfo:
    name: str
    dbus_type: str | None
    is_deprecated: bool
    dbus_deprecated: bool
    is_secret: bool
    is_secret_flags: bool


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate Rust NetworkManager connection-profile models from pinned NM source.",
    )
    parser.add_argument(
        "--nm-source",
        type=Path,
        required=True,
        help="Path to the pinned NetworkManager source tree.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help=f"Output Rust module path (default: {DEFAULT_OUTPUT}).",
    )
    return parser.parse_args()


def normalize_setting_key(setting_name: str) -> str:
    return re.sub(r"[^A-Za-z0-9]", "_", setting_name).upper()


def normalize_property_key(property_name: str) -> str:
    return re.sub(r"[^A-Za-z0-9]", "_", property_name).upper()


def parse_descriptions(header_path: Path) -> dict[tuple[str, str], str]:
    descriptions: dict[tuple[str, str], str] = {}
    pattern = re.compile(
        r'^#define\s+DESCRIBE_DOC_NM_SETTING_(?P<setting>[A-Z0-9_]+)_(?P<prop>[A-Z0-9_]+)\s+N_\("(?P<desc>.*)"\)$'
    )

    for raw_line in header_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        match = pattern.match(line)
        if not match:
            continue
        desc = match.group("desc")
        desc = desc.replace(r'\"', '"')
        descriptions[(match.group("setting"), match.group("prop"))] = desc

    return descriptions


def parse_metadata(xml_path: Path) -> dict[str, list[PropertyInfo]]:
    root = ET.fromstring(xml_path.read_text(encoding="utf-8"))
    settings: dict[str, list[PropertyInfo]] = {}

    for setting in root.findall("setting"):
        setting_name = setting.attrib["name"]
        properties: list[PropertyInfo] = []
        for prop in setting.findall("property"):
            properties.append(
                PropertyInfo(
                    name=prop.attrib["name"],
                    dbus_type=prop.attrib.get("dbus-type"),
                    is_deprecated=prop.attrib.get("is-deprecated") == "1",
                    dbus_deprecated=prop.attrib.get("dbus-deprecated") == "1",
                    is_secret=prop.attrib.get("is-secret") == "1",
                    is_secret_flags=prop.attrib.get("is-secret-flags") == "1",
                )
            )
        settings[setting_name] = properties

    return settings


def rust_field_name(setting_name: str, property_name: str) -> str:
    override = FIELD_NAME_OVERRIDES.get((setting_name, property_name))
    if override:
        return override

    candidate = property_name.replace("-", "_")
    if candidate in RUST_KEYWORDS:
        return f"{candidate}_field"
    return candidate


def rust_field_type(setting_name: str, property_info: PropertyInfo) -> str:
    override = TYPE_OVERRIDES.get((setting_name, property_info.name))
    if override:
        return override

    if property_info.dbus_type is None:
        raise ValueError(f"Missing dbus-type for {setting_name}.{property_info.name}")

    rust_type = DBUS_TYPE_MAP.get(property_info.dbus_type)
    if rust_type is None:
        raise ValueError(
            f"Unhandled dbus-type {property_info.dbus_type!r} for {setting_name}.{property_info.name}"
        )

    if (setting_name, property_info.name) in REQUIRED_FIELDS and rust_type.startswith("Option<"):
        return rust_type[len("Option<") : -1]
    return rust_type


def doc_lines(text: str) -> list[str]:
    wrapped: list[str] = []
    for paragraph in text.split("\n\n"):
        paragraph = paragraph.strip()
        if not paragraph:
            continue
        wrapped.extend(textwrap.wrap(paragraph, width=96))
        wrapped.append("")
    if wrapped and wrapped[-1] == "":
        wrapped.pop()
    return wrapped


def emit_property_docs(setting_name: str, property_info: PropertyInfo, descriptions: dict[tuple[str, str], str]) -> list[str]:
    key = (normalize_setting_key(setting_name), normalize_property_key(property_info.name))
    description = descriptions.get(key, "")
    notes: list[str] = []
    if property_info.is_secret:
        notes.append("Secret value.")
    if property_info.is_secret_flags:
        notes.append("Secret-flags field.")
    if property_info.is_deprecated:
        notes.append("Deprecated by NetworkManager.")
    elif property_info.dbus_deprecated:
        notes.append("Deprecated on D-Bus.")

    if notes:
        note_text = " ".join(notes)
        description = f"{description} {note_text}".strip()

    if not description:
        return []

    return [f"/// {line}" if line else "///" for line in doc_lines(description)]


def emit_struct(setting_name: str, properties: list[PropertyInfo], descriptions: dict[tuple[str, str], str]) -> str:
    struct_name = STRUCT_NAMES[setting_name]
    lines = [
        "#[derive(Clone, Deserialize, Serialize)]",
        "#[cfg_attr(test, derive(Debug, PartialEq))]",
        "#[cfg_attr(feature = \"api-docs\", derive(utoipa::ToSchema))]",
        f"pub struct {struct_name} {{",
    ]

    for property_info in properties:
        lines.extend(f"    {line}" for line in emit_property_docs(setting_name, property_info, descriptions))
        field_name = rust_field_name(setting_name, property_info.name)
        field_type = rust_field_type(setting_name, property_info)
        if field_name != property_info.name.replace("-", "_") or "-" in property_info.name:
            lines.append(f"    #[serde(rename = \"{property_info.name}\")]" )
        if (setting_name, property_info.name) in STRING_OR_ARRAY_FIELDS:
            lines.append("    #[serde(default, deserialize_with = \"deserialize_optional_string_vec\")]")
        lines.append(f"    pub {field_name}: {field_type},")

    lines.append("}")
    return "\n".join(lines)


def emit_top_level() -> str:
    lines = [
        "#[derive(Clone, Deserialize, Serialize)]",
        "#[cfg_attr(test, derive(Debug, PartialEq))]",
        "#[cfg_attr(feature = \"api-docs\", derive(utoipa::ToSchema))]",
        "pub struct ConnectionProfile {",
    ]

    for setting_name, field_name, optional in TOP_LEVEL_FIELDS:
        struct_name = STRUCT_NAMES[setting_name]
        if field_name != setting_name.replace("-", "_") or "-" in setting_name:
            lines.append(f"    #[serde(rename = \"{setting_name}\")]" )
        type_name = f"Option<{struct_name}>" if optional else struct_name
        lines.append(f"    pub {field_name}: {type_name},")

    lines.append("    pub activated: Option<bool>,")
    lines.append("}")
    return "\n".join(lines)


def render_module(settings: dict[str, list[PropertyInfo]], descriptions: dict[tuple[str, str], str], nm_source: Path) -> str:
    parts = [
        "//",
        "// SPDX-License-Identifier: LicenseRef-Ezurio-Clause",
        "// Copyright (C) 2026 Ezurio LLC.",
        "//",
        "// @generated by tools/generate_nm_connection_profile.py",
        f"// Source: {nm_source}",
        "",
        "use serde::{Deserialize, Deserializer, Serialize};",
        "use serde_json::Value;",
        "use std::collections::BTreeMap;",
        "",
        "fn deserialize_optional_string_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>",
        "where",
        "    D: Deserializer<'de>,",
        "{",
        "    #[derive(Deserialize)]",
        "    #[serde(untagged)]",
        "    enum StringVecOrString {",
        "        Many(Vec<String>),",
        "        One(String),",
        "    }",
        "",
        "    Ok(match Option::<StringVecOrString>::deserialize(deserializer)? {",
        "        None => None,",
        "        Some(StringVecOrString::Many(values)) => Some(values),",
        "        Some(StringVecOrString::One(value)) => Some(vec![value]),",
        "    })",
        "}",
        "",
    ]

    for setting_name in TARGET_SETTINGS:
        parts.append(emit_struct(setting_name, settings[setting_name], descriptions))
        parts.append("")

    parts.extend(
        [
            emit_top_level(),
            "",
            "#[derive(Clone, Deserialize, Serialize)]",
            "#[cfg_attr(test, derive(Debug, PartialEq))]",
            "#[cfg_attr(feature = \"api-docs\", derive(utoipa::ToSchema))]",
            "pub struct LegacyConnectionProfileResponse {",
            "    #[serde(rename = \"SDCERR\")]",
            "    pub sdcerr: i32,",
            "    #[serde(rename = \"InfoMsg\")]",
            "    pub info_msg: String,",
            "    #[serde(skip_serializing_if = \"Option::is_none\")]",
            "    pub connection: Option<ConnectionProfile>,",
            "}",
            "",
            "#[derive(Clone, Deserialize, Serialize)]",
            "#[cfg_attr(test, derive(Debug, PartialEq))]",
            "#[cfg_attr(feature = \"api-docs\", derive(utoipa::ToSchema))]",
            "pub struct ActivateConnectionLegacyRequest {",
            "    pub uuid: String,",
            "    pub activate: i32,",
            "}",
            "",
            "#[cfg(test)]",
            "mod tests {",
            "    use super::ConnectionProfile;",
            "    use serde_json::json;",
            "    use std::collections::BTreeMap;",
            "",
            "    #[test]",
            "    fn connection_profile_accepts_scalar_or_array_string_fields() {",
            "        let profile: ConnectionProfile = serde_json::from_value(json!({",
            "            \"connection\": {",
            "                \"permissions\": \"user:test:\",",
            "                \"secondaries\": \"vpn-uuid\",",
            "                \"ip-ping-addresses\": \"1.1.1.1\"",
            "            },",
            "            \"802-1x\": {",
            "                \"eap\": \"peap\",",
            "                \"altsubject-matches\": \"DNS:example.com\",",
            "                \"phase2-auth\": \"mschapv2\",",
            "                \"phase2-autheap\": \"tls\",",
            "                \"phase2-altsubject-matches\": \"DNS:inner.example.com\"",
            "            },",
            "            \"ipv4\": {",
            "                \"dhcp-reject-servers\": \"10.0.0.1\",",
            "                \"dns-data\": \"8.8.8.8\",",
            "                \"dns-options\": \"rotate\",",
            "                \"dns-search\": \"example.com\"",
            "            },",
            "            \"ipv6\": {",
            "                \"dhcp-reject-servers\": \"fd00::1\",",
            "                \"dns\": \"2001:4860:4860::8888\",",
            "                \"dns-data\": \"2001:4860:4860::8844\",",
            "                \"dns-options\": \"attempts:2\",",
            "                \"dns-search\": \"example.net\"",
            "            },",
            "            \"802-3-ethernet\": {",
            "                \"mac-address-blacklist\": \"00:11:22:33:44:55\",",
            "                \"mac-address-denylist\": \"66:77:88:99:aa:bb\",",
            "                \"s390-subchannels\": \"0.0.f500\"",
            "            },",
            "            \"802-11-wireless\": {",
            "                \"ssid\": \"test-ssid\",",
            "                \"mac-address-blacklist\": \"00:11:22:33:44:66\",",
            "                \"mac-address-denylist\": \"66:77:88:99:aa:cc\",",
            "                \"seen-bssids\": \"de:ad:be:ef:00:01\"",
            "            },",
            "            \"802-11-wireless-security\": {",
            "                \"key-mgmt\": \"wpa-psk\",",
            "                \"group\": \"tkip\",",
            "                \"pairwise\": \"ccmp\",",
            "                \"proto\": \"rsn\"",
            "            }",
            "        }))",
            "        .expect(\"connection profile should deserialize\");",
            "",
            "        assert_eq!(profile.connection.permissions, Some(vec![\"user:test:\".to_string()]));",
            "        assert_eq!(profile.connection.secondaries, Some(vec![\"vpn-uuid\".to_string()]));",
            "        assert_eq!(profile.connection.ip_ping_addresses, Some(vec![\"1.1.1.1\".to_string()]));",
            "",
            "        let enterprise_auth = profile.enterprise_auth.expect(\"enterprise auth section\");",
            "        assert_eq!(enterprise_auth.eap, Some(vec![\"peap\".to_string()]));",
            "        assert_eq!(enterprise_auth.altsubject_matches, Some(vec![\"DNS:example.com\".to_string()]));",
            "        assert_eq!(enterprise_auth.phase2_auth, Some(vec![\"mschapv2\".to_string()]));",
            "        assert_eq!(enterprise_auth.phase2_autheap, Some(vec![\"tls\".to_string()]));",
            "        assert_eq!(enterprise_auth.phase2_altsubject_matches, Some(vec![\"DNS:inner.example.com\".to_string()]));",
            "",
            "        let ipv4 = profile.ipv4.expect(\"ipv4 section\");",
            "        assert_eq!(ipv4.dhcp_reject_servers, Some(vec![\"10.0.0.1\".to_string()]));",
            "        assert_eq!(ipv4.dns_data, Some(vec![\"8.8.8.8\".to_string()]));",
            "        assert_eq!(ipv4.dns_options, Some(vec![\"rotate\".to_string()]));",
            "        assert_eq!(ipv4.dns_search, Some(vec![\"example.com\".to_string()]));",
            "",
            "        let ipv6 = profile.ipv6.expect(\"ipv6 section\");",
            "        assert_eq!(ipv6.dhcp_reject_servers, Some(vec![\"fd00::1\".to_string()]));",
            "        assert_eq!(ipv6.dns, Some(vec![\"2001:4860:4860::8888\".to_string()]));",
            "        assert_eq!(ipv6.dns_data, Some(vec![\"2001:4860:4860::8844\".to_string()]));",
            "        assert_eq!(ipv6.dns_options, Some(vec![\"attempts:2\".to_string()]));",
            "        assert_eq!(ipv6.dns_search, Some(vec![\"example.net\".to_string()]));",
            "",
            "        let wired = profile.wired.expect(\"wired section\");",
            "        assert_eq!(wired.mac_address_blacklist, Some(vec![\"00:11:22:33:44:55\".to_string()]));",
            "        assert_eq!(wired.mac_address_denylist, Some(vec![\"66:77:88:99:aa:bb\".to_string()]));",
            "        assert_eq!(wired.s390_subchannels, Some(vec![\"0.0.f500\".to_string()]));",
            "",
            "        let wireless = profile.wireless.expect(\"wireless section\");",
            "        assert_eq!(wireless.mac_address_blacklist, Some(vec![\"00:11:22:33:44:66\".to_string()]));",
            "        assert_eq!(wireless.mac_address_denylist, Some(vec![\"66:77:88:99:aa:cc\".to_string()]));",
            "        assert_eq!(wireless.seen_bssids, Some(vec![\"de:ad:be:ef:00:01\".to_string()]));",
            "",
            "        let wireless_security = profile.wireless_security.expect(\"wireless security section\");",
            "        assert_eq!(wireless_security.group, Some(vec![\"tkip\".to_string()]));",
            "        assert_eq!(wireless_security.pairwise, Some(vec![\"ccmp\".to_string()]));",
            "        assert_eq!(wireless_security.proto, Some(vec![\"rsn\".to_string()]));",
            "    }",
            "",
            "    #[test]",
            "    fn connection_profile_accepts_weblcm_style_payload() {",
            "        let profile: ConnectionProfile = serde_json::from_value(json!({",
            "            \"connection\": {",
            "                \"id\": \"CorpWifi\",",
            "                \"type\": \"802-11-wireless\",",
            "                \"interface-name\": \"wlan0\",",
            "                \"uuid\": \"123e4567-e89b-12d3-a456-426614174000\"",
            "            },",
            "            \"802-11-wireless\": {",
            "                \"ssid\": \"CorpWifi\",",
            "                \"mode\": \"infrastructure\",",
            "                \"security\": \"802-11-wireless-security\",",
            "                \"seen-bssids\": \"de:ad:be:ef:00:01\"",
            "            },",
            "            \"802-11-wireless-security\": {",
            "                \"key-mgmt\": \"wpa-eap\",",
            "                \"pairwise\": \"ccmp\",",
            "                \"group\": \"tkip\",",
            "                \"proto\": \"rsn\"",
            "            },",
            "            \"802-1x\": {",
            "                \"eap\": \"peap\",",
            "                \"identity\": \"user@example.com\",",
            "                \"phase2-auth\": \"mschapv2\",",
            "                \"altsubject-matches\": \"DNS:auth.example.com\"",
            "            },",
            "            \"ipv4\": {",
            "                \"method\": \"auto\",",
            "                \"address-data\": [{\"address\": \"10.10.10.20\", \"prefix\": 24}],",
            "                \"dhcp-reject-servers\": \"10.10.10.1\",",
            "                \"dns-data\": \"8.8.8.8\",",
            "                \"dns-options\": \"rotate\",",
            "                \"dns-search\": \"example.com\",",
            "                \"route-data\": [{\"dest\": \"0.0.0.0\", \"prefix\": 0, \"next-hop\": \"10.10.10.1\", \"metric\": 100}],",
            "                \"routing-rules\": [{\"priority\": 1000, \"from\": \"10.10.10.0/24\", \"table\": 100}]",
            "            },",
            "            \"ipv6\": {",
            "                \"method\": \"auto\",",
            "                \"address-data\": [{\"address\": \"fd00::20\", \"prefix\": 64}],",
            "                \"dns\": \"2001:4860:4860::8888\",",
            "                \"dns-search\": \"example.net\",",
            "                \"route-data\": [{\"dest\": \"::\", \"prefix\": 0, \"next-hop\": \"fd00::1\", \"metric\": 100}],",
            "                \"routing-rules\": [{\"priority\": 1001, \"from\": \"fd00::/64\", \"table\": 101}]",
            "            },",
            "            \"wifi-p2p\": {",
            "                \"device-name\": \"Summit P2P\",",
            "                \"frequency\": 2412,",
            "                \"peer\": \"aa:bb:cc:dd:ee:ff\",",
            "                \"peer-device-name\": \"Peer Device\"",
            "            }",
            "        }))",
            "        .expect(\"weblcm-style connection profile should deserialize\");",
            "",
            "        let wireless = profile.wireless.expect(\"wireless section\");",
            "        assert_eq!(wireless.seen_bssids, Some(vec![\"de:ad:be:ef:00:01\".to_string()]));",
            "        assert_eq!(wireless.security, Some(json!(\"802-11-wireless-security\")));",
            "",
            "        let wireless_security = profile.wireless_security.expect(\"wireless security section\");",
            "        assert_eq!(wireless_security.pairwise, Some(vec![\"ccmp\".to_string()]));",
            "        assert_eq!(wireless_security.group, Some(vec![\"tkip\".to_string()]));",
            "        assert_eq!(wireless_security.proto, Some(vec![\"rsn\".to_string()]));",
            "",
            "        let enterprise_auth = profile.enterprise_auth.expect(\"enterprise auth section\");",
            "        assert_eq!(enterprise_auth.eap, Some(vec![\"peap\".to_string()]));",
            "        assert_eq!(enterprise_auth.phase2_auth, Some(vec![\"mschapv2\".to_string()]));",
            "        assert_eq!(enterprise_auth.altsubject_matches, Some(vec![\"DNS:auth.example.com\".to_string()]));",
            "",
            "        let ipv4 = profile.ipv4.expect(\"ipv4 section\");",
            "        assert_eq!(ipv4.dhcp_reject_servers, Some(vec![\"10.10.10.1\".to_string()]));",
            "        assert_eq!(ipv4.dns_data, Some(vec![\"8.8.8.8\".to_string()]));",
            "        assert_eq!(ipv4.dns_options, Some(vec![\"rotate\".to_string()]));",
            "        assert_eq!(ipv4.dns_search, Some(vec![\"example.com\".to_string()]));",
            "        assert_eq!(ipv4.address_data, Some(vec![BTreeMap::from([",
            "            (\"address\".to_string(), json!(\"10.10.10.20\")),",
            "            (\"prefix\".to_string(), json!(24)),",
            "        ])]));",
            "        assert_eq!(ipv4.route_data, Some(vec![BTreeMap::from([",
            "            (\"dest\".to_string(), json!(\"0.0.0.0\")),",
            "            (\"prefix\".to_string(), json!(0)),",
            "            (\"next-hop\".to_string(), json!(\"10.10.10.1\")),",
            "            (\"metric\".to_string(), json!(100)),",
            "        ])]));",
            "        assert_eq!(ipv4.routing_rules, Some(vec![BTreeMap::from([",
            "            (\"priority\".to_string(), json!(1000)),",
            "            (\"from\".to_string(), json!(\"10.10.10.0/24\")),",
            "            (\"table\".to_string(), json!(100)),",
            "        ])]));",
            "",
            "        let ipv6 = profile.ipv6.expect(\"ipv6 section\");",
            "        assert_eq!(ipv6.dns, Some(vec![\"2001:4860:4860::8888\".to_string()]));",
            "        assert_eq!(ipv6.dns_search, Some(vec![\"example.net\".to_string()]));",
            "        assert_eq!(ipv6.address_data, Some(vec![BTreeMap::from([",
            "            (\"address\".to_string(), json!(\"fd00::20\")),",
            "            (\"prefix\".to_string(), json!(64)),",
            "        ])]));",
            "        assert_eq!(ipv6.route_data, Some(vec![BTreeMap::from([",
            "            (\"dest\".to_string(), json!(\"::\")),",
            "            (\"prefix\".to_string(), json!(0)),",
            "            (\"next-hop\".to_string(), json!(\"fd00::1\")),",
            "            (\"metric\".to_string(), json!(100)),",
            "        ])]));",
            "        assert_eq!(ipv6.routing_rules, Some(vec![BTreeMap::from([",
            "            (\"priority\".to_string(), json!(1001)),",
            "            (\"from\".to_string(), json!(\"fd00::/64\")),",
            "            (\"table\".to_string(), json!(101)),",
            "        ])]));",
            "",
            "        let wifi_p2p = profile.wifi_p2p.expect(\"wifi-p2p section\");",
            "        assert_eq!(wifi_p2p.device_name.as_deref(), Some(\"Summit P2P\"));",
            "        assert_eq!(wifi_p2p.frequency, Some(2412));",
            "        assert_eq!(wifi_p2p.peer.as_deref(), Some(\"aa:bb:cc:dd:ee:ff\"));",
            "        assert_eq!(wifi_p2p.peer_device_name.as_deref(), Some(\"Peer Device\"));",
            "    }",
            "}",
            "",
        ]
    )

    return "\n".join(parts)


def main() -> int:
    args = parse_args()
    nm_source = args.nm_source.resolve()
    output = args.output.resolve()

    metadata_path = nm_source / "src/libnm-core-impl/gen-metadata-nm-settings-libnm-core.xml.in"
    descriptions_path = nm_source / "src/libnmc-setting/settings-docs.h.in"

    if not metadata_path.is_file():
        raise FileNotFoundError(f"Missing metadata file: {metadata_path}")
    if not descriptions_path.is_file():
        raise FileNotFoundError(f"Missing descriptions file: {descriptions_path}")

    descriptions = parse_descriptions(descriptions_path)
    settings = parse_metadata(metadata_path)

    missing_settings = [setting_name for setting_name in TARGET_SETTINGS if setting_name not in settings]
    if missing_settings:
        raise ValueError(f"Missing settings in metadata: {', '.join(missing_settings)}")

    rendered = render_module(settings, descriptions, nm_source)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(rendered, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())