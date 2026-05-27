//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Direct nl80211 helpers intended to replace shell-based wireless probes.

use anyhow::{Context, Result, anyhow, bail};
use neli::attr::{AttrHandle, Attribute};
use neli::consts::{
    genl::{Cmd, NlAttrType},
    nl::{NlmF, NlmFFlags, Nlmsg},
    socket::NlFamily,
};
use neli::genl::{AttrType, Genlmsghdr, Nlattr};
use neli::nl::{NlPayload, Nlmsghdr};
use neli::socket::NlSocketHandle;
use neli::types::{Buffer, GenlBuffer};
use neli_proc_macros::neli_enum;
use std::collections::{BTreeMap, BTreeSet};

const NL_80211_GENL_NAME: &str = "nl80211";
const NL_80211_GENL_VERSION: u8 = 1;
const PRIMARY_WIPHY: u32 = 0;
const NO_IR_FLAG: u32 = 1 << 7;
const DFS_FLAG: u32 = 1 << 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nl80211Interface {
    pub ifindex: u32,
    pub wiphy: u32,
    pub name: String,
    pub frequency: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableApChannel {
    pub channel: u32,
    pub frequency: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StationRateInfo {
    pub rate: Option<i64>,
    pub channel_width: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StationInfo {
    pub signal: Option<i64>,
    pub inactive: Option<i64>,
    pub connected_time: Option<i64>,
    pub rx_packets: Option<i64>,
    pub tx_packets: Option<i64>,
    pub beacon_rx: Option<i64>,
    pub rx_rate: Option<StationRateInfo>,
    pub tx_rate: Option<StationRateInfo>,
    pub rx_bytes: Option<i64>,
    pub tx_bytes: Option<i64>,
    pub rx_duration: Option<i64>,
    pub tx_retries: Option<i64>,
    pub tx_failed: Option<i64>,
    pub beacon_loss: Option<i64>,
    pub rx_drop_misc: Option<i64>,
    pub dtim_period: Option<i64>,
    pub beacon_interval: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RegulatoryRule {
    start_mhz: u32,
    end_mhz: u32,
    flags: u32,
}

pub struct Nl80211Client {
    sock: NlSocketHandle,
    family_id: u16,
}

#[neli_enum(serialized_type = "u8")]
enum Nl80211Cmd {
    CmdUnspec = 0,
    CmdGetWiphy = 1,
    CmdNewWiphy = 3,
    CmdGetInterface = 5,
    CmdNewInterface = 7,
    CmdDelInterface = 8,
    CmdGetStation = 17,
    CmdNewStation = 19,
    CmdGetReg = 31,
}

impl Cmd for Nl80211Cmd {}

#[neli_enum(serialized_type = "u16")]
enum Nl80211Attr {
    AttrUnspec = 0,
    AttrWiphy = 1,
    AttrWiphyName = 2,
    AttrIfindex = 3,
    AttrIfname = 4,
    AttrIftype = 5,
    AttrMac = 6,
    AttrStaInfo = 21,
    AttrWiphyBands = 22,
    AttrRegAlpha2 = 33,
    AttrRegRules = 34,
    AttrWiphyFreq = 38,
    AttrSplitWiphyDump = 174,
}

impl NlAttrType for Nl80211Attr {}

#[neli_enum(serialized_type = "u16")]
enum Nl80211Iftype {
    IftypeUnspecified = 0,
    IftypeStation = 2,
}

impl NlAttrType for Nl80211Iftype {}

#[neli_enum(serialized_type = "u16")]
enum Nl80211StaInfo {
    StaInfoInvalid = 0,
    StaInfoInactiveTime = 1,
    StaInfoTxBitrate = 8,
    StaInfoRxPackets = 9,
    StaInfoTxPackets = 10,
    StaInfoTxRetries = 11,
    StaInfoTxFailed = 12,
    StaInfoRxBitrate = 14,
    StaInfoBssParam = 15,
    StaInfoConnectedTime = 16,
    StaInfoBeaconLoss = 18,
    StaInfoSignal = 7,
    StaInfoRxBytes64 = 23,
    StaInfoTxBytes64 = 24,
    StaInfoRxDropMisc = 28,
    StaInfoBeaconRx = 29,
    StaInfoRxDuration = 32,
}

impl NlAttrType for Nl80211StaInfo {}

#[neli_enum(serialized_type = "u16")]
enum Nl80211StaBssParam {
    StaBssParamInvalid = 0,
    StaBssParamDtimPeriod = 4,
    StaBssParamBeaconInterval = 5,
}

impl NlAttrType for Nl80211StaBssParam {}

#[neli_enum(serialized_type = "u16")]
enum Nl80211RateInfo {
    RateInfoInvalid = 0,
    RateInfoBitrate = 1,
    RateInfo40MhzWidth = 3,
    RateInfoBitrate32 = 5,
    RateInfo80MhzWidth = 8,
    RateInfo80p80MhzWidth = 9,
    RateInfo160MhzWidth = 10,
    RateInfo10MhzWidth = 11,
    RateInfo5MhzWidth = 12,
}

impl NlAttrType for Nl80211RateInfo {}

#[neli_enum(serialized_type = "u16")]
enum Nl80211BandAttr {
    BandAttrInvalid = 0,
    BandAttrFreqs = 1,
}

impl NlAttrType for Nl80211BandAttr {}

#[neli_enum(serialized_type = "u16")]
enum Nl80211FrequencyAttr {
    FrequencyAttrInvalid = 0,
    FrequencyAttrFreq = 1,
    FrequencyAttrDisabled = 2,
}

impl NlAttrType for Nl80211FrequencyAttr {}

#[neli_enum(serialized_type = "u16")]
enum Nl80211RegRuleAttr {
    RegRuleAttrInvalid = 0,
    AttrRegRuleFlags = 1,
    AttrFreqRangeStart = 2,
    AttrFreqRangeEnd = 3,
}

impl NlAttrType for Nl80211RegRuleAttr {}

impl Nl80211Client {
    pub fn connect() -> Result<Self> {
        let mut sock =
            NlSocketHandle::connect(NlFamily::Generic, None, &[]).map_err(|error| anyhow!(error))?;
        sock.nonblock().map_err(|error| anyhow!(error))?;
        let family_id = sock
            .resolve_genl_family(NL_80211_GENL_NAME)
            .map_err(|error| anyhow!(error))?;
        Ok(Self { sock, family_id })
    }

    pub fn list_interfaces(&mut self) -> Result<Vec<Nl80211Interface>> {
        let genl = Genlmsghdr::<Nl80211Cmd, Nl80211Attr>::new(
            Nl80211Cmd::CmdGetInterface,
            NL_80211_GENL_VERSION,
            GenlBuffer::new(),
        );
        let msg = Nlmsghdr::new(
            None,
            self.family_id,
            NlmFFlags::new(&[NlmF::Request, NlmF::Dump]),
            None,
            None,
            NlPayload::Payload(genl),
        );

        self.sock.send(msg).map_err(|error| anyhow!(error))?;

        let mut interfaces = Vec::new();
        let iter = self
            .sock
            .iter::<Nlmsg, Genlmsghdr<Nl80211Cmd, Nl80211Attr>>(false);

        for response in iter {
            let response = response.map_err(|error| anyhow!(error))?;
            match response.nl_type {
                Nlmsg::Noop => continue,
                Nlmsg::Done => break,
                Nlmsg::Error => bail!("nl80211 GET_INTERFACE failed"),
                _ => {}
            }

            let Some(payload) = response.nl_payload.get_payload() else {
                continue;
            };
            if payload.cmd != Nl80211Cmd::CmdNewInterface {
                continue;
            }

            let handle = payload.get_attr_handle();
            let wiphy = get_required_attr::<u32, _>(&handle, Nl80211Attr::AttrWiphy)?;
            let ifindex = get_required_attr::<u32, _>(&handle, Nl80211Attr::AttrIfindex)?;
            let ifname = trim_c_string(get_required_attr_bytes(&handle, Nl80211Attr::AttrIfname)?)?;
            let frequency = get_optional_attr::<u32, _>(&handle, Nl80211Attr::AttrWiphyFreq)?;

            interfaces.push(Nl80211Interface {
                ifindex,
                wiphy,
                name: ifname,
                frequency,
            });
        }

        Ok(interfaces)
    }

    pub fn get_interface(&mut self, ifname: &str) -> Result<Nl80211Interface> {
        self.list_interfaces()?
            .into_iter()
            .find(|interface| interface.name == ifname)
            .ok_or_else(|| anyhow!("interface not found"))
    }

    pub fn get_reg_domain_primary(&mut self) -> Result<String> {
        let attrs = genl_buffer(vec![
            Nlattr::new(false, false, Nl80211Attr::AttrWiphy, PRIMARY_WIPHY)
                .map_err(|error| anyhow!(error))?,
        ]);
        let genl = Genlmsghdr::<Nl80211Cmd, Nl80211Attr>::new(
            Nl80211Cmd::CmdGetReg,
            NL_80211_GENL_VERSION,
            attrs,
        );
        let msg = Nlmsghdr::new(
            None,
            self.family_id,
            NlmFFlags::new(&[NlmF::Request]),
            None,
            None,
            NlPayload::Payload(genl),
        );

        self.sock.send(msg).map_err(|error| anyhow!(error))?;

        let iter = self
            .sock
            .iter::<Nlmsg, Genlmsghdr<Nl80211Cmd, Nl80211Attr>>(false);
        for response in iter {
            let response = response.map_err(|error| anyhow!(error))?;
            match response.nl_type {
                Nlmsg::Noop => continue,
                Nlmsg::Done => break,
                Nlmsg::Error => bail!("nl80211 GET_REG failed"),
                _ => {}
            }

            let Some(payload) = response.nl_payload.get_payload() else {
                continue;
            };
            let handle = payload.get_attr_handle();
            if let Ok(alpha2) = get_required_attr_bytes(&handle, Nl80211Attr::AttrRegAlpha2)
                .and_then(trim_c_string)
            {
                return Ok(alpha2);
            }
        }

        bail!("primary regulatory domain not found")
    }

    pub fn get_frequency_info(&mut self, ifname: &str) -> Result<u32> {
        self.get_interface(ifname)?
            .frequency
            .ok_or_else(|| anyhow!("interface frequency not found"))
    }

    pub fn get_active_ap_rssi(&mut self, ifname: &str) -> Result<f64> {
        let stations = self.get_station_dump(ifname)?;
        stations
            .into_values()
            .find_map(|station| station.signal.map(|value| value as f64))
            .ok_or_else(|| anyhow!("station signal not found"))
    }

    pub fn get_station_dump(&mut self, ifname: &str) -> Result<BTreeMap<String, StationInfo>> {
        let interface = self.get_interface(ifname)?;
        let attrs = genl_buffer(vec![
            Nlattr::new(false, false, Nl80211Attr::AttrIfindex, interface.ifindex)
                .map_err(|error| anyhow!(error))?,
        ]);
        let genl = Genlmsghdr::<Nl80211Cmd, Nl80211Attr>::new(
            Nl80211Cmd::CmdGetStation,
            NL_80211_GENL_VERSION,
            attrs,
        );
        let msg = Nlmsghdr::new(
            None,
            self.family_id,
            NlmFFlags::new(&[NlmF::Request, NlmF::Dump]),
            None,
            None,
            NlPayload::Payload(genl),
        );

        self.sock.send(msg).map_err(|error| anyhow!(error))?;

        let mut stations = BTreeMap::new();
        let iter = self
            .sock
            .iter::<Nlmsg, Genlmsghdr<Nl80211Cmd, Nl80211Attr>>(false);
        for response in iter {
            let response = response.map_err(|error| anyhow!(error))?;
            match response.nl_type {
                Nlmsg::Noop => continue,
                Nlmsg::Done => break,
                Nlmsg::Error => bail!("nl80211 GET_STATION failed"),
                _ => {}
            }

            let Some(payload) = response.nl_payload.get_payload() else {
                continue;
            };
            if payload.cmd != Nl80211Cmd::CmdNewStation {
                continue;
            }

            let handle = payload.get_attr_handle();
            let mac = format_mac(&get_required_attr_bytes(&handle, Nl80211Attr::AttrMac)?);
            let sta_attr = handle
                .get_attribute(Nl80211Attr::AttrStaInfo)
                .context("station response missing STA_INFO")?;
            let sta_handle: AttrHandle<
                '_,
                GenlBuffer<Nl80211StaInfo, Buffer>,
                Nlattr<Nl80211StaInfo, Buffer>,
            > = sta_attr.get_attr_handle().map_err(|error| anyhow!(error))?;

            stations.insert(mac, parse_station_info(&sta_handle)?);
        }

        Ok(stations)
    }

    pub fn get_available_ap_channels(&mut self, ifname: &str) -> Result<Vec<AvailableApChannel>> {
        let interface = self.get_interface(ifname)?;
        let mut supported = self.get_supported_frequencies(interface.wiphy)?;
        let reg_rules = self.get_regulatory_rules(interface.wiphy)?;

        supported.retain(|frequency| {
            !reg_rules.iter().any(|rule| {
                rule.start_mhz <= *frequency
                    && *frequency <= rule.end_mhz
                    && (rule.flags & (DFS_FLAG | NO_IR_FLAG)) != 0
            })
        });

        Ok(supported
            .into_iter()
            .map(|frequency| AvailableApChannel {
                channel: crate::utils::frequency_to_channel(frequency),
                frequency,
            })
            .collect())
    }

    pub fn add_virtual_interface(&mut self, ifname: &str) -> Result<()> {
        let attrs = genl_buffer(vec![
            Nlattr::new(false, false, Nl80211Attr::AttrWiphy, PRIMARY_WIPHY)
                .map_err(|error| anyhow!(error))?,
            Nlattr::new(
                false,
                false,
                Nl80211Attr::AttrIfname,
                c_string_bytes(ifname),
            )
            .map_err(|error| anyhow!(error))?,
            Nlattr::new(
                false,
                false,
                Nl80211Attr::AttrIftype,
                u32::from(u16::from(Nl80211Iftype::IftypeStation)),
            )
            .map_err(|error| anyhow!(error))?,
        ]);
        self.send_ack_command(Nl80211Cmd::CmdNewInterface, attrs)
    }

    pub fn remove_virtual_interface(&mut self, ifname: &str) -> Result<bool> {
        let interface = match self.get_interface(ifname) {
            Ok(interface) => interface,
            Err(_) => return Ok(false),
        };
        let attrs = genl_buffer(vec![
            Nlattr::new(false, false, Nl80211Attr::AttrIfindex, interface.ifindex)
                .map_err(|error| anyhow!(error))?,
        ]);
        self.send_ack_command(Nl80211Cmd::CmdDelInterface, attrs)?;
        Ok(true)
    }

    fn get_supported_frequencies(&mut self, wiphy: u32) -> Result<Vec<u32>> {
        let attrs = genl_buffer(vec![
            Nlattr::new(false, false, Nl80211Attr::AttrWiphy, wiphy)
                .map_err(|error| anyhow!(error))?,
            Nlattr {
                nla_len: 4,
                nla_type: AttrType {
                    nla_nested: false,
                    nla_network_order: false,
                    nla_type: Nl80211Attr::AttrSplitWiphyDump,
                },
                nla_payload: Buffer::new(),
            },
        ]);
        let genl = Genlmsghdr::<Nl80211Cmd, Nl80211Attr>::new(
            Nl80211Cmd::CmdGetWiphy,
            NL_80211_GENL_VERSION,
            attrs,
        );
        let msg = Nlmsghdr::new(
            None,
            self.family_id,
            NlmFFlags::new(&[NlmF::Request, NlmF::Ack, NlmF::Root, NlmF::Match]),
            None,
            Some(42069),
            NlPayload::Payload(genl),
        );

        self.sock.send(msg).map_err(|error| anyhow!(error))?;

        let mut frequencies = BTreeSet::new();
        let iter = self
            .sock
            .iter::<Nlmsg, Genlmsghdr<Nl80211Cmd, Nl80211Attr>>(false);
        for response in iter {
            let response = response.map_err(|error| anyhow!(error))?;
            match response.nl_type {
                Nlmsg::Noop => continue,
                Nlmsg::Done => break,
                Nlmsg::Error => bail!("nl80211 GET_WIPHY failed"),
                _ => {}
            }

            let Some(payload) = response.nl_payload.get_payload() else {
                continue;
            };
            if payload.cmd != Nl80211Cmd::CmdNewWiphy {
                continue;
            }

            let handle = payload.get_attr_handle();
            for attr in handle.iter() {
                if attr.nla_type.nla_type != Nl80211Attr::AttrWiphyBands {
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
                        if band_attr.nla_type.nla_type != Nl80211BandAttr::BandAttrFreqs {
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
                            let frequency = get_optional_attr::<u32, _>(
                                &freq_handle,
                                Nl80211FrequencyAttr::FrequencyAttrFreq,
                            )?;
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
        }

        Ok(frequencies.into_iter().collect())
    }

    fn get_regulatory_rules(&mut self, wiphy: u32) -> Result<Vec<RegulatoryRule>> {
        let attrs = genl_buffer(vec![
            Nlattr::new(false, false, Nl80211Attr::AttrWiphy, wiphy)
                .map_err(|error| anyhow!(error))?,
        ]);
        let genl = Genlmsghdr::<Nl80211Cmd, Nl80211Attr>::new(
            Nl80211Cmd::CmdGetReg,
            NL_80211_GENL_VERSION,
            attrs,
        );
        let msg = Nlmsghdr::new(
            None,
            self.family_id,
            NlmFFlags::new(&[NlmF::Request]),
            None,
            None,
            NlPayload::Payload(genl),
        );

        self.sock.send(msg).map_err(|error| anyhow!(error))?;

        let mut rules = Vec::new();
        let iter = self
            .sock
            .iter::<Nlmsg, Genlmsghdr<Nl80211Cmd, Nl80211Attr>>(false);
        for response in iter {
            let response = response.map_err(|error| anyhow!(error))?;
            match response.nl_type {
                Nlmsg::Noop => continue,
                Nlmsg::Done => break,
                Nlmsg::Error => bail!("nl80211 GET_REG failed"),
                _ => {}
            }

            let Some(payload) = response.nl_payload.get_payload() else {
                continue;
            };
            let handle = payload.get_attr_handle();
            let Some(reg_rules_attr) = handle.get_attribute(Nl80211Attr::AttrRegRules) else {
                continue;
            };

            let reg_rules: AttrHandle<'_, GenlBuffer<u16, Buffer>, Nlattr<u16, Buffer>> =
                reg_rules_attr.get_attr_handle().map_err(|error| anyhow!(error))?;
            for reg_rule in reg_rules.get_attrs() {
                let rule_handle: AttrHandle<
                    '_,
                    GenlBuffer<Nl80211RegRuleAttr, Buffer>,
                    Nlattr<Nl80211RegRuleAttr, Buffer>,
                > = reg_rule.get_attr_handle().map_err(|error| anyhow!(error))?;
                let start_khz = get_required_attr::<u32, _>(
                    &rule_handle,
                    Nl80211RegRuleAttr::AttrFreqRangeStart,
                )?;
                let end_khz = get_required_attr::<u32, _>(
                    &rule_handle,
                    Nl80211RegRuleAttr::AttrFreqRangeEnd,
                )?;
                let flags = get_required_attr::<u32, _>(
                    &rule_handle,
                    Nl80211RegRuleAttr::AttrRegRuleFlags,
                )?;
                rules.push(RegulatoryRule {
                    start_mhz: start_khz / 1000,
                    end_mhz: end_khz / 1000,
                    flags,
                });
            }
        }

        Ok(rules)
    }

    fn send_ack_command(
        &mut self,
        cmd: Nl80211Cmd,
        attrs: GenlBuffer<Nl80211Attr, Buffer>,
    ) -> Result<()> {
        let genl = Genlmsghdr::<Nl80211Cmd, Nl80211Attr>::new(cmd, NL_80211_GENL_VERSION, attrs);
        let msg = Nlmsghdr::new(
            None,
            self.family_id,
            NlmFFlags::new(&[NlmF::Request, NlmF::Ack]),
            None,
            None,
            NlPayload::Payload(genl),
        );

        self.sock.send(msg).map_err(|error| anyhow!(error))?;

        let iter = self
            .sock
            .iter::<Nlmsg, Genlmsghdr<Nl80211Cmd, Nl80211Attr>>(true);
        for response in iter.flatten() {
            match response.nl_type {
                Nlmsg::Noop => continue,
                Nlmsg::Done => break,
                Nlmsg::Error => match response.nl_payload {
                    NlPayload::Ack(_) => continue,
                    NlPayload::Err(error) => return Err(anyhow!(error.to_string())),
                    NlPayload::Payload(payload) => {
                        return Err(anyhow!(format!("unexpected payload: {payload:?}")));
                    }
                    NlPayload::Empty => return Err(anyhow!("empty netlink payload")),
                },
                _ => {}
            }
        }
        Ok(())
    }
}

fn parse_station_info(
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
        rx_bytes: get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoRxBytes64)?.map(i64::try_from).transpose().map_err(|error| anyhow!(error))?,
        tx_bytes: get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoTxBytes64)?.map(i64::try_from).transpose().map_err(|error| anyhow!(error))?,
        rx_duration: get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoRxDuration)?.map(i64::try_from).transpose().map_err(|error| anyhow!(error))?,
        tx_retries: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoTxRetries)?.map(i64::from),
        tx_failed: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoTxFailed)?.map(i64::from),
        beacon_loss: get_optional_attr::<u32, _>(handle, Nl80211StaInfo::StaInfoBeaconLoss)?.map(i64::from),
        rx_drop_misc: get_optional_attr::<u64, _>(handle, Nl80211StaInfo::StaInfoRxDropMisc)?.map(i64::try_from).transpose().map_err(|error| anyhow!(error))?,
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

fn parse_rate_info(
    handle: &AttrHandle<'_, GenlBuffer<Nl80211RateInfo, Buffer>, Nlattr<Nl80211RateInfo, Buffer>>,
) -> Result<StationRateInfo> {
    let rate = if let Some(rate) = get_optional_attr::<u32, _>(handle, Nl80211RateInfo::RateInfoBitrate32)? {
        Some(i64::from(rate) * 100)
    } else {
        get_optional_attr::<u16, _>(handle, Nl80211RateInfo::RateInfoBitrate)?
            .map(|rate| i64::from(rate) * 100)
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

fn genl_buffer<K>(attrs: Vec<Nlattr<K, Buffer>>) -> GenlBuffer<K, Buffer>
where
    K: NlAttrType,
{
    let mut buffer = GenlBuffer::new();
    for attr in attrs {
        buffer.push(attr);
    }
    buffer
}

fn get_required_attr<T, K>(
    handle: &AttrHandle<'_, GenlBuffer<K, Buffer>, Nlattr<K, Buffer>>,
    key: K,
) -> Result<T>
where
    T: for<'a> neli::FromBytes<'a>,
    K: Copy + NlAttrType,
{
    handle
        .get_attribute(key)
        .context("missing netlink attribute")?
        .get_payload_as()
        .map_err(|error| anyhow!(error))
}

fn get_optional_attr<T, K>(
    handle: &AttrHandle<'_, GenlBuffer<K, Buffer>, Nlattr<K, Buffer>>,
    key: K,
) -> Result<Option<T>>
where
    T: for<'a> neli::FromBytes<'a>,
    K: Copy + NlAttrType,
{
    handle
        .get_attribute(key)
        .map(|attr| attr.get_payload_as().map_err(|error| anyhow!(error)))
        .transpose()
}

fn get_required_attr_bytes<K>(
    handle: &AttrHandle<'_, GenlBuffer<K, Buffer>, Nlattr<K, Buffer>>,
    key: K,
) -> Result<Vec<u8>>
where
    K: Copy + NlAttrType,
{
    handle
        .get_attribute(key)
        .context("missing netlink bytes attribute")?
        .get_payload_as_with_len()
        .map_err(|error| anyhow!(error))
}

fn trim_c_string(value: Vec<u8>) -> Result<String> {
    let value = value
        .into_iter()
        .take_while(|byte| *byte != 0)
        .collect::<Vec<_>>();
    String::from_utf8(value).map_err(|error| anyhow!(error))
}

fn c_string_bytes(value: &str) -> Vec<u8> {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn format_mac(bytes: &[u8]) -> String {
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
