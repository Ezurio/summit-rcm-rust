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
use neli::genl::Nlattr;
use neli::nl::NlPayload;
use neli::router::asynchronous::{NlRouter, NlRouterReceiverHandle};
use neli::types::{Buffer, GenlBuffer};
use neli::utils::Groups;
use std::collections::BTreeMap;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::time::sleep;

use self::parsing::parse_station_info;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
use self::parsing::{RegulatoryRule, parse_regulatory_rules, parse_supported_frequencies};
use self::protocol::{
    NL_80211_GENL_NAME, Nl80211Attr, Nl80211Cmd, Nl80211Iftype, Nl80211RawPayload,
    Nl80211StaInfo, build_genl_message, c_string_bytes,
    format_mac, genl_buffer, get_optional_attr_raw, get_required_attr_bytes_raw, get_required_attr_raw, nlattr,
    nl80211_attrs, nl80211_commands, nl80211_iftype,
    nl80211_attr, nl80211_cmd, trim_c_string,
};

const NL_80211_RECV_TIMEOUT: Duration = Duration::from_secs(10);
const PRIMARY_WIPHY: u32 = 0;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const NO_IR_FLAG: u32 = 1 << 7;
#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
const DFS_FLAG: u32 = 1 << 4;

type Nl80211RecvHandle = NlRouterReceiverHandle<u16, Nl80211RawPayload>;
type Nl80211RawMsg = neli::nl::Nlmsghdr<u16, Nl80211RawPayload>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nl80211Interface {
    pub ifindex: u32,
    pub wiphy: u32,
    pub name: String,
    pub frequency: Option<u32>,
}

#[cfg(any(feature = "api-v2", feature = "api-legacy"))]
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
    router: NlRouter,
    family_id: u16,
}

impl Nl80211Client {
    pub async fn connect() -> Result<Self> {
        let (router, _) =
            NlRouter::connect(NlFamily::Generic, None, Groups::empty()).await.map_err(|error| anyhow!(error))?;
        let family_id = router.resolve_genl_family(NL_80211_GENL_NAME).await.map_err(|error| anyhow!(error))?;
        Ok(Self { router, family_id })
    }

    async fn recv_message_with_timeout<T, P>(
        &mut self,
        recv: &mut NlRouterReceiverHandle<T, P>,
    ) -> Result<Option<neli::nl::Nlmsghdr<T, P>>>
    where
        T: neli::consts::nl::NlType,
        P: neli::Size + neli::FromBytesWithInput<Input = usize>,
    {
        let recv_message = recv.next::<T, P>();
        tokio::pin!(recv_message);
        let timeout = sleep(NL_80211_RECV_TIMEOUT);
        tokio::pin!(timeout);

        tokio::select! {
            result = &mut recv_message => match result {
                Some(Ok(message)) => Ok(Some(message)),
                Some(Err(error)) => Err(anyhow!(error.to_string())),
                None => Ok(None),
            },
            _ = &mut timeout => Err(anyhow!("nl80211 recv timed out after {}s", NL_80211_RECV_TIMEOUT.as_secs())),
            _ = crate::utils::wait_for_shutdown() => Err(anyhow!("nl80211 recv cancelled")),
        }
    }

    pub async fn list_interfaces(&mut self) -> Result<Vec<Nl80211Interface>> {
        let genl = build_genl_message(
            nl80211_cmd(nl80211_commands::NL80211_CMD_GET_INTERFACE),
            GenlBuffer::new(),
        )?;
        let mut recv: Nl80211RecvHandle = self
            .router
            .send(self.family_id, NlmF::REQUEST | NlmF::DUMP, NlPayload::Payload(genl))
            .await
            .map_err(|error| anyhow!(error))?;

        let mut interfaces = Vec::new();
        let mut done = false;
        while !done {
            let Some(response) = self.recv_message_with_timeout(&mut recv).await? else {
                break;
            };
            let Some(payload) = response_payload(&response, "GET_INTERFACE", &mut done)? else {
                continue;
            };
            if *payload.cmd() != nl80211_cmd(nl80211_commands::NL80211_CMD_NEW_INTERFACE) {
                continue;
            }

            let handle = payload.attrs().get_attr_handle();

            let Ok(wiphy) = get_required_attr_raw::<u32, _>(
                &handle,
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_WIPHY),
            ) else {
                continue;
            };
            let Ok(ifindex) = get_required_attr_raw::<u32, _>(
                &handle,
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_IFINDEX),
            ) else {
                continue;
            };
            let Ok(ifname_raw) = get_required_attr_bytes_raw(
                &handle,
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_IFNAME),
            ) else {
                continue;
            };
            let ifname = trim_c_string(ifname_raw)?;
            let frequency = get_optional_attr_raw::<u32, _>(
                &handle,
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_WIPHY_FREQ),
            )?;

            interfaces.push(Nl80211Interface {
                ifindex,
                wiphy,
                name: ifname,
                frequency,
            });
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
            nlattr(
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_WIPHY),
                PRIMARY_WIPHY,
            )?,
        ]);
        let genl = build_genl_message(
            nl80211_cmd(nl80211_commands::NL80211_CMD_GET_REG),
            attrs,
        )?;
        let mut recv: Nl80211RecvHandle = self
            .router
            .send(self.family_id, NlmF::REQUEST, NlPayload::Payload(genl))
            .await
            .map_err(|error| anyhow!(error))?;

        let mut done = false;
        while !done {
            let Some(response) = self.recv_message_with_timeout(&mut recv).await? else {
                break;
            };
            let Some(payload) = response_payload(&response, "GET_REG", &mut done)? else {
                continue;
            };
            let handle = payload.attrs().get_attr_handle();
            if let Ok(alpha2) =
                get_required_attr_bytes_raw(
                    &handle,
                    nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_REG_ALPHA2),
                )
                .and_then(trim_c_string)
            {
                return Ok(alpha2);
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
            nlattr(
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_IFINDEX),
                interface.ifindex,
            )?,
        ]);
        let genl = build_genl_message(
            nl80211_cmd(nl80211_commands::NL80211_CMD_GET_STATION),
            attrs,
        )?;
        let mut recv: Nl80211RecvHandle = self
            .router
            .send(self.family_id, NlmF::REQUEST | NlmF::DUMP, NlPayload::Payload(genl))
            .await
            .map_err(|error| anyhow!(error))?;

        let mut stations = BTreeMap::new();
        let mut done = false;
        while !done {
            let Some(response) = self.recv_message_with_timeout(&mut recv).await? else {
                break;
            };

            let Some(payload) = response_payload(&response, "GET_STATION", &mut done)? else {
                continue;
            };

            if *payload.cmd() != nl80211_cmd(nl80211_commands::NL80211_CMD_NEW_STATION) {
                continue;
            }

            let handle = payload.attrs().get_attr_handle();
            let mac = format_mac(
                &get_required_attr_bytes_raw(
                    &handle,
                    nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_MAC),
                )
                    .context("GET_STATION missing NL80211_ATTR_MAC")?,
            );

            let sta_attr = handle
                .get_attribute(
                    nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_STA_INFO),
                )
                .context("GET_STATION missing NL80211_ATTR_STA_INFO")?;
            let sta_handle: AttrHandle<
                '_,
                GenlBuffer<Nl80211StaInfo, Buffer>,
                Nlattr<Nl80211StaInfo, Buffer>,
            > = sta_attr.get_attr_handle().map_err(|error| anyhow!(error))?;

            stations.insert(mac, parse_station_info(&sta_handle)?);
        }

        Ok(stations)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    pub async fn get_available_ap_channels(&mut self, ifname: &str) -> Result<Vec<AvailableApChannel>> {
        let interface = self.get_interface(ifname).await?;
        let mut supported = self
            .get_supported_frequencies(interface.wiphy)
            .await
            .with_context(|| format!("GET_WIPHY supported frequencies for wiphy {}", interface.wiphy))?;
        let reg_rules = self
            .get_regulatory_rules(interface.wiphy)
            .await
            .with_context(|| format!("GET_REG regulatory rules for wiphy {}", interface.wiphy))?;

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
            nlattr(
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_WIPHY),
                PRIMARY_WIPHY,
            )?,
            nlattr(
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_IFNAME),
                c_string_bytes(ifname),
            )?,
            nlattr(
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_IFTYPE),
                nl80211_attr::<Nl80211Iftype>(nl80211_iftype::NL80211_IFTYPE_STATION)
                    as u32,
            )?,
        ]);
        self.send_ack_command(
            nl80211_cmd(nl80211_commands::NL80211_CMD_NEW_INTERFACE),
            attrs,
        )
        .await
    }

    pub async fn remove_virtual_interface(&mut self, ifname: &str) -> Result<bool> {
        let interface = match self.get_interface(ifname).await {
            Ok(interface) => interface,
            Err(_) => return Ok(false),
        };
        let attrs = genl_buffer(vec![
            nlattr(
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_IFINDEX),
                interface.ifindex,
            )?,
        ]);
        self.send_ack_command(
            nl80211_cmd(nl80211_commands::NL80211_CMD_DEL_INTERFACE),
            attrs,
        )
        .await?;
        Ok(true)
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    async fn get_supported_frequencies(&mut self, wiphy: u32) -> Result<Vec<u32>> {
        let attrs = genl_buffer(vec![
            nlattr(
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_SPLIT_WIPHY_DUMP),
                (),
            )?,
        ]);
        let genl = build_genl_message(
            nl80211_cmd(nl80211_commands::NL80211_CMD_GET_WIPHY),
            attrs,
        )?;
        let mut recv: Nl80211RecvHandle = self
            .router
            .send(
                self.family_id,
                NlmF::DUMP | NlmF::ACK,
                NlPayload::Payload(genl),
            )
            .await
            .map_err(|error| anyhow!(error))?;

        let mut frequencies = BTreeSet::new();
        let mut done = false;
        while !done {
            let response = self.recv_message_with_timeout(&mut recv).await?;
            let Some(response) = response else {
                break;
            };
            let Some(payload) = response_payload(&response, "GET_WIPHY", &mut done)? else {
                continue;
            };
            if *payload.cmd() != nl80211_cmd(nl80211_commands::NL80211_CMD_NEW_WIPHY) {
                continue;
            }

            let handle = payload.attrs().get_attr_handle();
            let Ok(response_wiphy) = get_required_attr_raw::<u32, _>(
                &handle,
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_WIPHY),
            ) else {
                continue;
            };
            if response_wiphy != wiphy {
                continue;
            }

            frequencies.extend(parse_supported_frequencies(&handle)?);
        }

        Ok(frequencies.into_iter().collect())
    }

    #[cfg(any(feature = "api-v2", feature = "api-legacy"))]
    async fn get_regulatory_rules(&mut self, wiphy: u32) -> Result<Vec<RegulatoryRule>> {
        let attrs = genl_buffer(vec![
            nlattr(
                nl80211_attr::<Nl80211Attr>(nl80211_attrs::NL80211_ATTR_WIPHY),
                wiphy,
            )?,
        ]);
        let genl = build_genl_message(
            nl80211_cmd(nl80211_commands::NL80211_CMD_GET_REG),
            attrs,
        )?;
        let mut recv: Nl80211RecvHandle = self
            .router
            .send(self.family_id, NlmF::REQUEST, NlPayload::Payload(genl))
            .await
            .map_err(|error| anyhow!(error))?;

        let mut rules = Vec::new();
        let mut done = false;
        while !done {
            let Some(response) = self.recv_message_with_timeout(&mut recv).await? else {
                break;
            };
            let Some(payload) = response_payload(&response, "GET_REG", &mut done)? else {
                continue;
            };
            rules.extend(parse_regulatory_rules(&payload.attrs().get_attr_handle())?);
        }

        Ok(rules)
    }

    async fn send_ack_command(
        &mut self,
        cmd: Nl80211Cmd,
        attrs: GenlBuffer<Nl80211Attr, Buffer>,
    ) -> Result<()> {
        let genl = build_genl_message(cmd, attrs)?;
        let mut recv: Nl80211RecvHandle = self
            .router
            .send(self.family_id, NlmF::REQUEST | NlmF::ACK, NlPayload::Payload(genl))
            .await
            .map_err(|error| anyhow!(error))?;

        let mut done = false;
        while !done {
            let Some(response) = self.recv_message_with_timeout(&mut recv).await? else {
                break;
            };
            response_payload(&response, "ACK", &mut done)?;
        }
        Ok(())
    }
}

fn response_payload<'a>(
    response: &'a Nl80211RawMsg,
    request_name: &str,
    done: &mut bool,
) -> Result<Option<&'a Nl80211RawPayload>> {
    match Nlmsg::from(*response.nl_type()) {
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
