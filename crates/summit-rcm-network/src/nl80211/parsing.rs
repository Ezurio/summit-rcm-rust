// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use anyhow::{Result, anyhow};
use neli::attr::AttrHandle;
use neli::genl::Nlattr;
use neli::types::{Buffer, GenlBuffer};
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use std::collections::BTreeSet;

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use super::protocol::{
    Nl80211Attr, Nl80211BandAttr, Nl80211FrequencyAttr, Nl80211RegRuleAttr, get_required_attr,
    nl80211_attrs, nl80211_band_attr, nl80211_frequency_attr, nl80211_reg_rule_attr,
};
use super::protocol::{
    Nl80211RateInfo, Nl80211StaBssParam, Nl80211StaInfo, get_optional_attr, has_attr, nl80211_attr,
    nl80211_rate_info, nl80211_sta_bss_param, nl80211_sta_info,
};
use super::{StationInfo, StationRateInfo};

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
        .get_attribute(nl80211_attr::<Nl80211StaInfo>(
            nl80211_sta_info::NL80211_STA_INFO_BSS_PARAM,
        ))
        .map(|attr| attr.get_attr_handle())
        .transpose()
        .map_err(|error| anyhow!(error))?;
    let rx_rate = handle
        .get_attribute(nl80211_attr::<Nl80211StaInfo>(
            nl80211_sta_info::NL80211_STA_INFO_RX_BITRATE,
        ))
        .map(|attr| attr.get_attr_handle())
        .transpose()
        .map_err(|error| anyhow!(error))?;
    let tx_rate = handle
        .get_attribute(nl80211_attr::<Nl80211StaInfo>(
            nl80211_sta_info::NL80211_STA_INFO_TX_BITRATE,
        ))
        .map(|attr| attr.get_attr_handle())
        .transpose()
        .map_err(|error| anyhow!(error))?;

    let signal = get_optional_attr::<i8, _>(
        handle,
        nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_SIGNAL),
    )?
    .map(i64::from);
    let inactive = get_optional_attr::<u32, _>(
        handle,
        nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_INACTIVE_TIME),
    )?
    .map(i64::from);
    let connected_time = get_optional_attr::<u32, _>(
        handle,
        nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_CONNECTED_TIME),
    )?
    .map(i64::from);
    let rx_bytes = optional_i64_from_u64_or_u32(
        get_optional_attr::<u64, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_RX_BYTES64),
        )?,
        get_optional_attr::<u32, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_RX_BYTES),
        )?,
    )?;
    let tx_bytes = optional_i64_from_u64_or_u32(
        get_optional_attr::<u64, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_TX_BYTES64),
        )?,
        get_optional_attr::<u32, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_TX_BYTES),
        )?,
    )?;

    Ok(StationInfo {
        signal,
        inactive,
        connected_time,
        rx_packets: get_optional_attr::<u32, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_RX_PACKETS),
        )?
        .map(i64::from),
        tx_packets: get_optional_attr::<u32, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_TX_PACKETS),
        )?
        .map(i64::from),
        beacon_rx: optional_i64_from_u64(get_optional_attr::<u64, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_BEACON_RX),
        )?)?,
        rx_rate: rx_rate.as_ref().map(parse_rate_info).transpose()?,
        tx_rate: tx_rate.as_ref().map(parse_rate_info).transpose()?,
        rx_bytes,
        tx_bytes,
        rx_duration: optional_i64_from_u64(get_optional_attr::<u64, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_RX_DURATION),
        )?)?,
        tx_retries: get_optional_attr::<u32, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_TX_RETRIES),
        )?
        .map(i64::from),
        tx_failed: get_optional_attr::<u32, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_TX_FAILED),
        )?
        .map(i64::from),
        beacon_loss: get_optional_attr::<u32, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_BEACON_LOSS),
        )?
        .map(i64::from),
        rx_drop_misc: optional_i64_from_u64(get_optional_attr::<u64, _>(
            handle,
            nl80211_attr::<Nl80211StaInfo>(nl80211_sta_info::NL80211_STA_INFO_RX_DROP_MISC),
        )?)?,
        dtim_period: bss_param
            .as_ref()
            .map(|nested| {
                get_optional_attr::<u8, _>(
                    nested,
                    nl80211_attr::<Nl80211StaBssParam>(
                        nl80211_sta_bss_param::NL80211_STA_BSS_PARAM_DTIM_PERIOD,
                    ),
                )
            })
            .transpose()?
            .flatten()
            .map(i64::from),
        beacon_interval: bss_param
            .as_ref()
            .map(|nested| {
                get_optional_attr::<u16, _>(
                    nested,
                    nl80211_attr::<Nl80211StaBssParam>(
                        nl80211_sta_bss_param::NL80211_STA_BSS_PARAM_BEACON_INTERVAL,
                    ),
                )
            })
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
        if *attr.nla_type().nla_type()
            != nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_WIPHY_BANDS)
        {
            continue;
        }

        let band_handle: AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>> =
            attr.get_attr_handle().map_err(|error| anyhow!(error))?;
        for band in band_handle.get_attrs() {
            let nested: AttrHandle<
                '_,
                GenlBuffer<Nl80211BandAttr, Buffer>,
                Nlattr<Nl80211BandAttr, Buffer>,
            > = band.get_attr_handle().map_err(|error| anyhow!(error))?;
            for band_attr in nested.get_attrs() {
                if *band_attr.nla_type().nla_type()
                    != nl80211_attr::<Nl80211BandAttr>(nl80211_band_attr::NL80211_BAND_ATTR_FREQS)
                {
                    continue;
                }

                let freq_list: AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>> =
                    band_attr
                        .get_attr_handle()
                        .map_err(|error| anyhow!(error))?;
                for freq in freq_list.get_attrs() {
                    let freq_handle: AttrHandle<
                        '_,
                        GenlBuffer<Nl80211FrequencyAttr, Buffer>,
                        Nlattr<Nl80211FrequencyAttr, Buffer>,
                    > = freq.get_attr_handle().map_err(|error| anyhow!(error))?;
                    let frequency = get_optional_attr::<u32, _>(
                        &freq_handle,
                        nl80211_attr::<Nl80211FrequencyAttr>(
                            nl80211_frequency_attr::NL80211_FREQUENCY_ATTR_FREQ,
                        ),
                    )?;
                    let disabled = has_attr(
                        &freq_handle,
                        nl80211_attr::<Nl80211FrequencyAttr>(
                            nl80211_frequency_attr::NL80211_FREQUENCY_ATTR_DISABLED,
                        ),
                    );

                    if let Some(frequency) = frequency.filter(|_| !disabled) {
                        let _ = frequencies.insert(frequency);
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
    let Some(reg_rules_attr) = handle.get_attribute(nl80211_attr::<Nl80211Attr>(
        nl80211_attrs::NL80211_ATTR_REG_RULES,
    )) else {
        return Ok(Vec::new());
    };

    let reg_rules: AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>> = reg_rules_attr
        .get_attr_handle()
        .map_err(|error| anyhow!(error))?;
    let mut rules = Vec::new();
    for reg_rule in reg_rules.get_attrs() {
        let rule_handle: AttrHandle<
            '_,
            GenlBuffer<Nl80211RegRuleAttr, Buffer>,
            Nlattr<Nl80211RegRuleAttr, Buffer>,
        > = reg_rule.get_attr_handle().map_err(|error| anyhow!(error))?;
        let start_khz = get_required_attr::<u32, _>(
            &rule_handle,
            nl80211_attr::<Nl80211RegRuleAttr>(
                nl80211_reg_rule_attr::NL80211_ATTR_FREQ_RANGE_START,
            ),
        )?;
        let end_khz = get_required_attr::<u32, _>(
            &rule_handle,
            nl80211_attr::<Nl80211RegRuleAttr>(nl80211_reg_rule_attr::NL80211_ATTR_FREQ_RANGE_END),
        )?;
        let flags = get_required_attr::<u32, _>(
            &rule_handle,
            nl80211_attr::<Nl80211RegRuleAttr>(nl80211_reg_rule_attr::NL80211_ATTR_REG_RULE_FLAGS),
        )?;
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
        get_optional_attr::<u32, _>(
            handle,
            nl80211_attr::<Nl80211RateInfo>(nl80211_rate_info::NL80211_RATE_INFO_BITRATE32),
        )?,
        get_optional_attr::<u16, _>(
            handle,
            nl80211_attr::<Nl80211RateInfo>(nl80211_rate_info::NL80211_RATE_INFO_BITRATE),
        )?,
    )?
    .map(|rate| rate * 100);

    let channel_width = if has_attr(
        handle,
        nl80211_attr::<Nl80211RateInfo>(nl80211_rate_info::NL80211_RATE_INFO_5_MHZ_WIDTH),
    ) {
        Some(5)
    } else if has_attr(
        handle,
        nl80211_attr::<Nl80211RateInfo>(nl80211_rate_info::NL80211_RATE_INFO_10_MHZ_WIDTH),
    ) {
        Some(10)
    } else if has_attr(
        handle,
        nl80211_attr::<Nl80211RateInfo>(nl80211_rate_info::NL80211_RATE_INFO_40_MHZ_WIDTH),
    ) {
        Some(40)
    } else if has_attr(
        handle,
        nl80211_attr::<Nl80211RateInfo>(nl80211_rate_info::NL80211_RATE_INFO_80_MHZ_WIDTH),
    ) {
        Some(80)
    } else if has_attr(
        handle,
        nl80211_attr::<Nl80211RateInfo>(nl80211_rate_info::NL80211_RATE_INFO_80P80_MHZ_WIDTH),
    ) || has_attr(
        handle,
        nl80211_attr::<Nl80211RateInfo>(nl80211_rate_info::NL80211_RATE_INFO_160_MHZ_WIDTH),
    ) {
        Some(160)
    } else {
        Some(20)
    };

    Ok(StationRateInfo {
        rate,
        channel_width,
    })
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
