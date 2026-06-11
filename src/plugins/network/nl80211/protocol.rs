// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

// Generated bindgen constants — allow dead_code since only a subset of the full
// nl80211 enum space is referenced at any given time.
#[allow(dead_code)]
mod nl80211_bindings {
    include!("generated.rs");
}
pub(super) use nl80211_bindings::*;

use anyhow::{Context, Result, anyhow};
use neli::attr::{AttrHandle, Attribute};
use neli::consts::genl::NlAttrType;
use neli::genl::{
    AttrTypeBuilder, Genlmsghdr, GenlmsghdrBuilder, Nlattr, NlattrBuilder,
};
use neli::types::{Buffer, GenlBuffer};

pub(super) const NL_80211_GENL_NAME: &str = "nl80211";
const NL_80211_GENL_VERSION: u8 = 1;

pub(super) type Nl80211Cmd = u8;
pub(super) type Nl80211Attr = u16;
pub(super) type Nl80211Iftype = u16;
pub(super) type Nl80211StaInfo = u16;
pub(super) type Nl80211StaBssParam = u16;
pub(super) type Nl80211RateInfo = u16;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub(super) type Nl80211BandAttr = u16;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub(super) type Nl80211FrequencyAttr = u16;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub(super) type Nl80211RegRuleAttr = u16;

pub(super) type Nl80211Payload = Genlmsghdr<Nl80211Cmd, Nl80211Attr>;
pub(super) type Nl80211RawPayload = Genlmsghdr<Nl80211Cmd, u16>;

#[inline]
pub(super) fn nl80211_cmd(value: u32) -> Nl80211Cmd {
    value as u8
}

#[inline]
pub(super) fn nl80211_attr<K>(value: u32) -> K
where
    K: From<u16>,
{
    K::from(value as u16)
}

pub(super) fn genl_buffer<K>(attrs: Vec<Nlattr<K, Buffer>>) -> GenlBuffer<K, Buffer>
where
    K: NlAttrType,
{
    let mut buffer = GenlBuffer::new();
    for attr in attrs {
        buffer.push(attr);
    }
    buffer
}

pub(super) fn nlattr<K, P>(key: K, payload: P) -> Result<Nlattr<K, Buffer>>
where
    K: Copy + NlAttrType + neli::Size,
    P: neli::Size + neli::ToBytes,
{
    NlattrBuilder::default()
        .nla_type(
            AttrTypeBuilder::default()
                .nla_type(key)
                .build()
                .map_err(|error| anyhow!(error))?,
        )
        .nla_payload(payload)
        .build()
        .map_err(|error| anyhow!(error))
}

pub(super) fn build_genl_message(
    cmd: Nl80211Cmd,
    attrs: GenlBuffer<Nl80211Attr, Buffer>,
) -> Result<Nl80211Payload> {
    GenlmsghdrBuilder::default()
        .cmd(cmd)
        .version(NL_80211_GENL_VERSION)
        .attrs(attrs)
        .build()
        .map_err(|error| anyhow!(error))
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub(super) fn get_required_attr<T, K>(
    handle: &AttrHandle<'_, GenlBuffer<K, Buffer>, Nlattr<K, Buffer>>,
    key: K,
) -> Result<T>
where
    T: neli::FromBytes,
    K: Copy + NlAttrType,
{
    handle
        .get_attribute(key)
        .context("missing netlink attribute")?
        .get_payload_as()
        .map_err(|error| anyhow!(error))
}

pub(super) fn get_required_attr_raw<T, K>(
    handle: &AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>>,
    key: K,
) -> Result<T>
where
    T: neli::FromBytes,
    K: Copy + Into<u16>,
{
    handle
        .get_attribute(key.into())
        .context("missing netlink attribute")?
        .get_payload_as()
        .map_err(|error| anyhow!(error))
}

pub(super) fn get_optional_attr<T, K>(
    handle: &AttrHandle<'_, GenlBuffer<K, Buffer>, Nlattr<K, Buffer>>,
    key: K,
) -> Result<Option<T>>
where
    T: neli::FromBytes,
    K: Copy + NlAttrType,
{
    handle
        .get_attribute(key)
        .map(|attr| attr.get_payload_as().map_err(|error| anyhow!(error)))
        .transpose()
}

pub(super) fn has_attr<K>(
    handle: &AttrHandle<'_, GenlBuffer<K, Buffer>, Nlattr<K, Buffer>>,
    key: K,
) -> bool
where
    K: Copy + NlAttrType,
{
    handle.get_attribute(key).is_some()
}

pub(super) fn get_optional_attr_raw<T, K>(
    handle: &AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>>,
    key: K,
) -> Result<Option<T>>
where
    T: neli::FromBytes,
    K: Copy + Into<u16>,
{
    handle
        .get_attribute(key.into())
        .map(|attr| attr.get_payload_as().map_err(|error| anyhow!(error)))
        .transpose()
}

pub(super) fn get_required_attr_bytes_raw<K>(
    handle: &AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>>,
    key: K,
) -> Result<Vec<u8>>
where
    K: Copy + Into<u16>,
{
    handle
        .get_attribute(key.into())
        .context("missing netlink bytes attribute")?
        .get_payload_as_with_len()
        .map_err(|error| anyhow!(error))
}

pub(super) fn trim_c_string(value: Vec<u8>) -> Result<String> {
    let value = value.into_iter().take_while(|byte| *byte != 0).collect::<Vec<_>>();
    String::from_utf8(value).map_err(|error| anyhow!(error))
}

pub(super) fn c_string_bytes(value: &str) -> Vec<u8> {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

pub(super) fn format_mac(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::{c_string_bytes, format_mac, trim_c_string};

    #[test]
    fn trim_c_string_strips_trailing_nul() {
        assert_eq!(trim_c_string(c_string_bytes("wlan0")).unwrap(), "wlan0");
    }

    #[test]
    fn format_mac_uses_lower_hex_octets() {
        assert_eq!(format_mac(&[0xc0, 0xee, 0x40, 0x43, 0xc4, 0x14]), "c0:ee:40:43:c4:14");
    }
}
