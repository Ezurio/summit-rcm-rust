// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! Direct nl80211 helpers intended to replace shell-based wireless probes.

mod parsing;
mod protocol;

use anyhow::{Context, Result, anyhow, bail};
use neli::attr::AttrHandle;
use neli::consts::{
    nl::{NlmF, Nlmsg},
    socket::NlFamily,
};
use neli::genl::{Genlmsghdr, Nlattr};
use neli::nl::NlPayload;
use neli::router::synchronous::NlRouter as SyncNlRouter;
use neli::socket::asynchronous::NlSocketHandle;
use neli::types::{Buffer, GenlBuffer};
use neli::utils::Groups;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use tokio::time::sleep;

use self::parsing::{RegulatoryRule, parse_regulatory_rules, parse_station_info, parse_supported_frequencies};
use self::protocol::{
    NL_80211_GENL_NAME, Nl80211Attr, Nl80211Cmd, Nl80211Iftype, Nl80211Payload, Nl80211Response,
    Nl80211Responses, Nl80211StaInfo, build_genl_message, build_nl_request, c_string_bytes,
    format_mac, genl_buffer, get_optional_attr, get_required_attr, get_required_attr_bytes, nlattr,
    trim_c_string,
};

const NL_80211_RECV_TIMEOUT: Duration = Duration::from_secs(10);
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

pub struct Nl80211Client {
    sock: NlSocketHandle,
    family_id: u16,
}

impl Nl80211Client {
    pub fn connect() -> Result<Self> {
        let sock =
            NlSocketHandle::connect(NlFamily::Generic, None, Groups::empty()).map_err(|error| anyhow!(error))?;
        let (router, _) =
            SyncNlRouter::connect(NlFamily::Generic, None, Groups::empty()).map_err(|error| anyhow!(error))?;
        let family_id = router.resolve_genl_family(NL_80211_GENL_NAME).map_err(|error| anyhow!(error))?;
        Ok(Self { sock, family_id })
    }

    async fn recv_messages(&mut self) -> Result<Nl80211Responses> {
        let (result, _) = self.sock.recv_all::<Nlmsg, Nl80211Payload>().await.map_err(|error| anyhow!(error))?;
        Ok(result)
    }

    async fn recv_messages_with_timeout(&mut self) -> Result<Nl80211Responses> {
        let recv_messages = self.recv_messages();
        tokio::pin!(recv_messages);
        let timeout = sleep(NL_80211_RECV_TIMEOUT);
        tokio::pin!(timeout);

        tokio::select! {
            result = &mut recv_messages => result,
            _ = &mut timeout => Err(anyhow!("nl80211 recv timed out after {}s", NL_80211_RECV_TIMEOUT.as_secs())),
            _ = crate::utils::wait_for_shutdown() => Err(anyhow!("nl80211 recv cancelled")),
        }
    }

    pub async fn list_interfaces(&mut self) -> Result<Vec<Nl80211Interface>> {
        let genl = build_genl_message(Nl80211Cmd::CmdGetInterface, GenlBuffer::new())?;
        let msg = build_nl_request(self.family_id, NlmF::REQUEST | NlmF::DUMP, None, genl)?;

        self.sock.send(&msg).await.map_err(|error| anyhow!(error))?;

        let mut interfaces = Vec::new();
        let mut done = false;
        while !done {
            let responses = self.recv_messages_with_timeout().await?;
            for response in responses {
                let Some(payload) = response_payload(&response, "GET_INTERFACE", &mut done)? else {
                    continue;
                };
                if *payload.cmd() != Nl80211Cmd::CmdNewInterface {
                    continue;
                }

                let handle = payload.attrs().get_attr_handle();
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
        }

        Ok(interfaces)
    }

    pub async fn get_interface(&mut self, ifname: &str) -> Result<Nl80211Interface> {
        self.list_interfaces()
            .await?
            .into_iter()
            .find(|interface| interface.name == ifname)
            .ok_or_else(|| anyhow!("interface not found"))
    }

    pub async fn get_reg_domain_primary(&mut self) -> Result<String> {
        let attrs = genl_buffer(vec![
            nlattr(Nl80211Attr::AttrWiphy, PRIMARY_WIPHY)?,
        ]);
        let genl = build_genl_message(Nl80211Cmd::CmdGetReg, attrs)?;
        let msg = build_nl_request(self.family_id, NlmF::REQUEST, None, genl)?;

        self.sock.send(&msg).await.map_err(|error| anyhow!(error))?;

        let mut done = false;
        while !done {
            let responses = self.recv_messages_with_timeout().await?;
            for response in responses {
                let Some(payload) = response_payload(&response, "GET_REG", &mut done)? else {
                    continue;
                };
                let handle = payload.attrs().get_attr_handle();
                if let Ok(alpha2) =
                    get_required_attr_bytes(&handle, Nl80211Attr::AttrRegAlpha2).and_then(trim_c_string)
                {
                    return Ok(alpha2);
                }
            }
        }

        bail!("primary regulatory domain not found")
    }

    pub async fn get_frequency_info(&mut self, ifname: &str) -> Result<u32> {
        self.get_interface(ifname)
            .await?
            .frequency
            .ok_or_else(|| anyhow!("interface frequency not found"))
    }

    pub async fn get_active_ap_rssi(&mut self, ifname: &str) -> Result<f64> {
        let stations = self.get_station_dump(ifname).await?;
        stations
            .into_values()
            .find_map(|station| station.signal.map(|value| value as f64))
            .ok_or_else(|| anyhow!("station signal not found"))
    }

    pub async fn get_station_dump(&mut self, ifname: &str) -> Result<BTreeMap<String, StationInfo>> {
        let interface = self.get_interface(ifname).await?;
        let attrs = genl_buffer(vec![
            nlattr(Nl80211Attr::AttrIfindex, interface.ifindex)?,
        ]);
        let genl = build_genl_message(Nl80211Cmd::CmdGetStation, attrs)?;
        let msg = build_nl_request(self.family_id, NlmF::REQUEST | NlmF::DUMP, None, genl)?;

        self.sock.send(&msg).await.map_err(|error| anyhow!(error))?;

        let mut stations = BTreeMap::new();
        let mut done = false;
        while !done {
            let responses = self.recv_messages_with_timeout().await?;
            for response in responses {
                let Some(payload) = response_payload(&response, "GET_STATION", &mut done)? else {
                    continue;
                };
                if *payload.cmd() != Nl80211Cmd::CmdNewStation {
                    continue;
                }

                let handle = payload.attrs().get_attr_handle();
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
        }

        Ok(stations)
    }

    pub async fn get_available_ap_channels(&mut self, ifname: &str) -> Result<Vec<AvailableApChannel>> {
        let interface = self.get_interface(ifname).await?;
        let mut supported = self.get_supported_frequencies(interface.wiphy).await?;
        let reg_rules = self.get_regulatory_rules(interface.wiphy).await?;

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

    pub async fn add_virtual_interface(&mut self, ifname: &str) -> Result<()> {
        let attrs = genl_buffer(vec![
            nlattr(Nl80211Attr::AttrWiphy, PRIMARY_WIPHY)?,
            nlattr(Nl80211Attr::AttrIfname, c_string_bytes(ifname))?,
            nlattr(
                Nl80211Attr::AttrIftype,
                u32::from(u16::from(Nl80211Iftype::IftypeStation)),
            )?,
        ]);
        self.send_ack_command(Nl80211Cmd::CmdNewInterface, attrs).await
    }

    pub async fn remove_virtual_interface(&mut self, ifname: &str) -> Result<bool> {
        let interface = match self.get_interface(ifname).await {
            Ok(interface) => interface,
            Err(_) => return Ok(false),
        };
        let attrs = genl_buffer(vec![
            nlattr(Nl80211Attr::AttrIfindex, interface.ifindex)?,
        ]);
        self.send_ack_command(Nl80211Cmd::CmdDelInterface, attrs).await?;
        Ok(true)
    }

    async fn get_supported_frequencies(&mut self, wiphy: u32) -> Result<Vec<u32>> {
        let attrs = genl_buffer(vec![
            nlattr(Nl80211Attr::AttrWiphy, wiphy)?,
            nlattr(Nl80211Attr::AttrSplitWiphyDump, Buffer::new())?,
        ]);
        let genl = build_genl_message(Nl80211Cmd::CmdGetWiphy, attrs)?;
        let msg = build_nl_request(
            self.family_id,
            NlmF::REQUEST | NlmF::ACK | NlmF::ROOT | NlmF::MATCH,
            Some(42069),
            genl,
        )?;

        self.sock.send(&msg).await.map_err(|error| anyhow!(error))?;

        let mut frequencies = BTreeSet::new();
        let mut done = false;
        while !done {
            let responses = self.recv_messages_with_timeout().await?;
            for response in responses {
                let Some(payload) = response_payload(&response, "GET_WIPHY", &mut done)? else {
                    continue;
                };
                if *payload.cmd() != Nl80211Cmd::CmdNewWiphy {
                    continue;
                }

                frequencies.extend(parse_supported_frequencies(&payload.attrs().get_attr_handle())?);
            }
        }

        Ok(frequencies.into_iter().collect())
    }

    async fn get_regulatory_rules(&mut self, wiphy: u32) -> Result<Vec<RegulatoryRule>> {
        let attrs = genl_buffer(vec![
            nlattr(Nl80211Attr::AttrWiphy, wiphy)?,
        ]);
        let genl = build_genl_message(Nl80211Cmd::CmdGetReg, attrs)?;
        let msg = build_nl_request(self.family_id, NlmF::REQUEST, None, genl)?;

        self.sock.send(&msg).await.map_err(|error| anyhow!(error))?;

        let mut rules = Vec::new();
        let mut done = false;
        while !done {
            let responses = self.recv_messages_with_timeout().await?;
            for response in responses {
                let Some(payload) = response_payload(&response, "GET_REG", &mut done)? else {
                    continue;
                };
                rules.extend(parse_regulatory_rules(&payload.attrs().get_attr_handle())?);
            }
        }

        Ok(rules)
    }

    async fn send_ack_command(
        &mut self,
        cmd: Nl80211Cmd,
        attrs: GenlBuffer<Nl80211Attr, Buffer>,
    ) -> Result<()> {
        let genl = build_genl_message(cmd, attrs)?;
        let msg = build_nl_request(self.family_id, NlmF::REQUEST | NlmF::ACK, None, genl)?;

        self.sock.send(&msg).await.map_err(|error| anyhow!(error))?;

        let mut done = false;
        while !done {
            let responses = self.recv_messages_with_timeout().await?;
            for response in responses {
                response_payload(&response, "ACK", &mut done)?;
            }
        }
        Ok(())
    }
}

fn response_payload<'a>(
    response: &'a Nl80211Response,
    request_name: &str,
    done: &mut bool,
) -> Result<Option<&'a Genlmsghdr<Nl80211Cmd, Nl80211Attr>>> {
    match *response.nl_type() {
        Nlmsg::Noop => return Ok(None),
        Nlmsg::Done => {
            *done = true;
            return Ok(None);
        }
        Nlmsg::Error => match response.nl_payload() {
            NlPayload::Ack(_) | NlPayload::DumpExtAck(_) => return Ok(None),
            NlPayload::Err(error) => {
                bail!("nl80211 {request_name} failed: {error}");
            }
            NlPayload::Payload(payload) => {
                bail!("nl80211 {request_name} returned unexpected payload: {payload:?}");
            }
            NlPayload::Empty => bail!("nl80211 {request_name} returned empty netlink payload"),
        },
        _ => {}
    }

    Ok(response.get_payload())
}