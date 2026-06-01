// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::{Result, anyhow};
use neli::attr::AttrHandle;
use neli::genl::Nlattr;
use neli::types::{Buffer, GenlBuffer};
use std::collections::BTreeSet;

use super::{StationInfo, StationRateInfo};
use super::protocol::{
    Nl80211Attr, Nl80211BandAttr, Nl80211FrequencyAttr, Nl80211RateInfo, Nl80211RegRuleAttr,
    Nl80211StaBssParam, Nl80211StaInfo, get_optional_attr, get_required_attr,
};

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

    Ok(StationInfo {
        signal: get_optional_attr::<i8, _>(handle, Nl80211StaInfo::StaInfoSignal)?.map(i64::from),
        inactive: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoInactiveTime)?.map(i64::from),
        connected_time: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoConnectedTime)?.map(i64::from),
        rx_packets: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoRxPackets)?.map(i64::from),
        tx_packets: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoTxPackets)?.map(i64::from),
        beacon_rx: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoBeaconRx)?.map(i64::from),
        rx_rate: rx_rate.as_ref().map(parse_rate_info).transpose()?,
        tx_rate: tx_rate.as_ref().map(parse_rate_info).transpose()?,
        rx_bytes: optional_i64_from_u64(get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoRxBytes64)?)?,
        tx_bytes: optional_i64_from_u64(get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoTxBytes64)?)?,
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

pub(super) fn parse_supported_frequencies(
    handle: &AttrHandle<'_, GenlBuffer<Nl80211Attr, Buffer>, Nlattr<Nl80211Attr, Buffer>>,
) -> Result<BTreeSet<u32>> {
    let mut frequencies = BTreeSet::new();

    for attr in handle.iter() {
        if *attr.nla_type().nla_type() != Nl80211Attr::AttrWiphyBands {
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
                    let disabled = get_optional_attr::<u8, _>(
                        &freq_handle,
                        Nl80211FrequencyAttr::FrequencyAttrDisabled,
                    )?
                    .is_some();

                    if let Some(frequency) = frequency.filter(|_| !disabled) {
                        frequencies.insert(frequency);
                    }
                }
            }
        }
    }

    Ok(frequencies)
}

pub(super) fn parse_regulatory_rules(
    handle: &AttrHandle<'_, GenlBuffer<Nl80211Attr, Buffer>, Nlattr<Nl80211Attr, Buffer>>,
) -> Result<Vec<RegulatoryRule>> {
    let Some(reg_rules_attr) = handle.get_attribute(Nl80211Attr::AttrRegRules) else {
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
    let rate = if let Some(rate) = get_optional_attr::<u32, _>(handle, Nl80211RateInfo::RateInfoBitrate32)? {
        Some(i64::from(rate) * 100)
    } else {
        get_optional_attr::<u16, _>(handle, Nl80211RateInfo::RateInfoBitrate)?.map(|rate| i64::from(rate) * 100)
    };

    let channel_width = if get_optional_attr::<u8, _>(handle, Nl80211RateInfo::RateInfo5MhzWidth)?.is_some() {
        Some(5)
    } else if get_optional_attr::<u8, _>(handle, Nl80211RateInfo::RateInfo10MhzWidth)?.is_some() {
        Some(10)
    } else if get_optional_attr::<u8, _>(handle, Nl80211RateInfo::RateInfo40MhzWidth)?.is_some() {
        Some(40)
    } else if get_optional_attr::<u8, _>(handle, Nl80211RateInfo::RateInfo80MhzWidth)?.is_some() {
        Some(80)
    } else if get_optional_attr::<u8, _>(handle, Nl80211RateInfo::RateInfo80p80MhzWidth)?.is_some()
        || get_optional_attr::<u8, _>(handle, Nl80211RateInfo::RateInfo160MhzWidth)?.is_some()
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