// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::{Result, anyhow};
use neli::attr::AttrHandle;
use neli::genl::Nlattr;
use neli::types::{Buffer, GenlBuffer};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use std::collections::BTreeSet;

use super::{StationInfo, StationRateInfo};
use super::protocol::{
    Nl80211RateInfo, Nl80211StaBssParam, Nl80211StaInfo, get_optional_attr, has_attr,
};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use super::protocol::{
    Nl80211Attr, Nl80211BandAttr, Nl80211FrequencyAttr, Nl80211RegRuleAttr,
    get_required_attr,
};

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RegulatoryRule {
    pub(super) start_mhz: u32,
    pub(super) end_mhz: u32,
    pub(super) flags: u32,
}

pub(super) fn parse_station_info(
    handle: &AttrHandle<'_, GenlBuffer<Nl80211StaInfo, Buffer>, Nlattr<Nl80211StaInfo, Buffer>>,
) -> Result<StationInfo> {
    let bss_param = handle
        .get_attribute(Nl80211StaInfo::StaInfoBssParam)
        .map(|attr| attr.get_attr_handle())
        .transpose()
        .map_err(|error| anyhow!(error))?;
    let rx_rate = handle
        .get_attribute(Nl80211StaInfo::StaInfoRxBitrate)
        .map(|attr| attr.get_attr_handle())
        .transpose()
        .map_err(|error| anyhow!(error))?;
    let tx_rate = handle
        .get_attribute(Nl80211StaInfo::StaInfoTxBitrate)
        .map(|attr| attr.get_attr_handle())
        .transpose()
        .map_err(|error| anyhow!(error))?;

    let signal = get_optional_attr::<i8, _>(handle, Nl80211StaInfo::StaInfoSignal)?.map(i64::from);
    let inactive = get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoInactiveTime)?.map(i64::from);
    let connected_time = get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoConnectedTime)?.map(i64::from);
    let rx_bytes = optional_i64_from_u64_or_u32(
        get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoRxBytes64)?,
        get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoRxBytes)?,
    )?;
    let tx_bytes = optional_i64_from_u64_or_u32(
        get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoTxBytes64)?,
        get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoTxBytes)?,
    )?;

    Ok(StationInfo {
        signal,
        inactive,
        connected_time,
        rx_packets: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoRxPackets)?.map(i64::from),
        tx_packets: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoTxPackets)?.map(i64::from),
        beacon_rx: optional_i64_from_u64(get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoBeaconRx)?)?,
        rx_rate: rx_rate.as_ref().map(parse_rate_info).transpose()?,
        tx_rate: tx_rate.as_ref().map(parse_rate_info).transpose()?,
        rx_bytes,
        tx_bytes,
        rx_duration: optional_i64_from_u64(get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoRxDuration)?)?,
        tx_retries: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoTxRetries)?.map(i64::from),
        tx_failed: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoTxFailed)?.map(i64::from),
        beacon_loss: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoBeaconLoss)?.map(i64::from),
        rx_drop_misc: optional_i64_from_u64(get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoRxDropMisc)?)?,
        dtim_period: bss_param
            .as_ref()
            .map(|nested| get_optional_attr::<u8, _>(nested, Nl80211StaBssParam::StaBssParamDtimPeriod))
            .transpose()?
            .flatten()
            .map(i64::from),
        beacon_interval: bss_param
            .as_ref()
            .map(|nested| get_optional_attr::<u16, _>(nested, Nl80211StaBssParam::StaBssParamBeaconInterval))
            .transpose()?
            .flatten()
            .map(i64::from),
    })
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub(super) fn parse_supported_frequencies(
    handle: &AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>>,
) -> Result<BTreeSet<u32>> {
    let mut frequencies = BTreeSet::new();

    for attr in handle.iter() {
        if *attr.nla_type().nla_type() != u16::from(Nl80211Attr::AttrWiphyBands) {
            continue;
        }

        let band_handle: AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>> =
            attr.get_attr_handle().map_err(|error| anyhow!(error))?;
        for band in band_handle.get_attrs() {
            let nested: AttrHandle<'_, GenlBuffer<Nl80211BandAttr, Buffer>, Nlattr<Nl80211BandAttr, Buffer>> =
                band.get_attr_handle().map_err(|error| anyhow!(error))?;
            for band_attr in nested.get_attrs() {
                if *band_attr.nla_type().nla_type() != Nl80211BandAttr::BandAttrFreqs {
                    continue;
                }

                let freq_list: AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>> =
                    band_attr.get_attr_handle().map_err(|error| anyhow!(error))?;
                for freq in freq_list.get_attrs() {
                    let freq_handle: AttrHandle<
                        '_,
                        GenlBuffer<Nl80211FrequencyAttr, Buffer>,
                        Nlattr<Nl80211FrequencyAttr, Buffer>,
                    > = freq.get_attr_handle().map_err(|error| anyhow!(error))?;
                    let frequency =
                        get_optional_attr::<u32, _>(&freq_handle, Nl80211FrequencyAttr::FrequencyAttrFreq)?;
                    let disabled = has_attr(&freq_handle, Nl80211FrequencyAttr::FrequencyAttrDisabled);

                    if let Some(frequency) = frequency.filter(|_| !disabled) {
                        frequencies.insert(frequency);
                    }
                }
            }
        }
    }

    Ok(frequencies)
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
pub(super) fn parse_regulatory_rules(
    handle: &AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>>,
) -> Result<Vec<RegulatoryRule>> {
    let Some(reg_rules_attr) = handle.get_attribute(u16::from(Nl80211Attr::AttrRegRules)) else {
        return Ok(Vec::new());
    };

    let reg_rules: AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>> =
        reg_rules_attr.get_attr_handle().map_err(|error| anyhow!(error))?;
    let mut rules = Vec::new();
    for reg_rule in reg_rules.get_attrs() {
        let rule_handle: AttrHandle<
            '_,
            GenlBuffer<Nl80211RegRuleAttr, Buffer>,
            Nlattr<Nl80211RegRuleAttr, Buffer>,
        > = reg_rule.get_attr_handle().map_err(|error| anyhow!(error))?;
        let start_khz = get_required_attr::<u32, _>(&rule_handle, Nl80211RegRuleAttr::AttrFreqRangeStart)?;
        let end_khz = get_required_attr::<u32, _>(&rule_handle, Nl80211RegRuleAttr::AttrFreqRangeEnd)?;
        let flags = get_required_attr::<u32, _>(&rule_handle, Nl80211RegRuleAttr::AttrRegRuleFlags)?;
        rules.push(RegulatoryRule {
            start_mhz: start_khz / 1000,
            end_mhz: end_khz / 1000,
            flags,
        });
    }

    Ok(rules)
}

fn parse_rate_info(
    handle: &AttrHandle<'_, GenlBuffer<Nl80211RateInfo, Buffer>, Nlattr<Nl80211RateInfo, Buffer>>,
) -> Result<StationRateInfo> {
    let rate = optional_i64_from_u32_or_u16(
        get_optional_attr::<u32, _>(handle, Nl80211RateInfo::RateInfoBitrate32)?,
        get_optional_attr::<u16, _>(handle, Nl80211RateInfo::RateInfoBitrate)?,
    )?
    .map(|rate| rate * 100);

    let channel_width = if has_attr(handle, Nl80211RateInfo::RateInfo5MhzWidth) {
        Some(5)
    } else if has_attr(handle, Nl80211RateInfo::RateInfo10MhzWidth) {
        Some(10)
    } else if has_attr(handle, Nl80211RateInfo::RateInfo40MhzWidth) {
        Some(40)
    } else if has_attr(handle, Nl80211RateInfo::RateInfo80MhzWidth) {
        Some(80)
    } else if has_attr(handle, Nl80211RateInfo::RateInfo80p80MhzWidth)
        || has_attr(handle, Nl80211RateInfo::RateInfo160MhzWidth)
    {
        Some(160)
    } else {
        Some(20)
    };

    Ok(StationRateInfo { rate, channel_width })
}

fn optional_i64_from_u64(value: Option<u64>) -> Result<Option<i64>> {
    value
        .map(i64::try_from)
        .transpose()
        .map_err(|error| anyhow!(error))
}

fn optional_i64_from_u64_or_u32(value64: Option<u64>, value32: Option<u32>) -> Result<Option<i64>> {
    if let Some(value) = value64 {
        Ok(Some(i64::try_from(value).map_err(|error| anyhow!(error))?))
    } else {
        Ok(value32.map(i64::from))
    }
}

fn optional_i64_from_u32_or_u16(value32: Option<u32>, value16: Option<u16>) -> Result<Option<i64>> {
    if let Some(value) = value32 {
        Ok(Some(i64::from(value)))
    } else {
        Ok(value16.map(i64::from))
    }
}
