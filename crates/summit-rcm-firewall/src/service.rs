//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! iptables-based firewall port forwarding service

use log::error;
use serde::{Deserialize, Serialize};
use summit_rcm_core::utils::command_output;
use summit_rcm_web::serde_json;
use tokio::sync::{Mutex as AsyncMutex, OnceCell, RwLock};

const IPTABLES: &str = "/usr/sbin/iptables";
const IP6TABLES: &str = "/usr/sbin/ip6tables";
pub(crate) const FORWARDED_PORTS_FILE: &str = "/tmp/summit-rcm.ports";
pub(crate) const ADD_PORT: &str = "addForwardPort";
pub(crate) const REMOVE_PORT: &str = "removeForwardPort";
#[cfg(feature = "api-legacy")]
pub(crate) const PORT_COMMANDS: &[&str] = &[ADD_PORT, REMOVE_PORT];
const WIFI_INTERFACE: &str = "wlan0";
pub(crate) const IPV4: &str = "ipv4";
pub(crate) const IPV6: &str = "ipv6";
pub(crate) const IP_VERSIONS: &[&str] = &[IPV4, IPV6];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ForwardedPort {
    pub port: u16,
    pub protocol: String,
    pub toport: String,
    pub toaddr: String,
    pub ip_version: String,
}

static FORWARDED_PORTS: RwLock<Vec<ForwardedPort>> = RwLock::const_new(Vec::new());
static PORTS_INTERLOCK: AsyncMutex<()> = AsyncMutex::const_new(());
static PORTS_LOADED: OnceCell<()> = OnceCell::const_new();

async fn with_ports_read<R>(f: impl FnOnce(&Vec<ForwardedPort>) -> R) -> R {
    let ports = FORWARDED_PORTS.read().await;
    f(&ports)
}

async fn with_ports<R>(f: impl FnOnce(&mut Vec<ForwardedPort>) -> R) -> R {
    let mut ports = FORWARDED_PORTS.write().await;
    f(&mut ports)
}

async fn ensure_ports_loaded() {
    let _ = PORTS_LOADED
        .get_or_init(|| async {
            let loaded = match tokio::fs::read_to_string(FORWARDED_PORTS_FILE).await {
                Ok(data) => serde_json::from_str::<Vec<ForwardedPort>>(&data).unwrap_or_default(),
                Err(_) => Vec::new(),
            };
            let mut ports = FORWARDED_PORTS.write().await;
            *ports = loaded;
        })
        .await;
}

pub(crate) struct FirewallService;

impl FirewallService {
    pub(crate) async fn get_forwarded_ports() -> Vec<ForwardedPort> {
        ensure_ports_loaded().await;
        with_ports_read(|ports| ports.clone()).await
    }

    pub(crate) async fn configure_forwarded_port(
        command: &str,
        fp: ForwardedPort,
    ) -> (bool, String) {
        ensure_ports_loaded().await;
        let _interlock = PORTS_INTERLOCK.lock().await;
        let present = with_ports_read(|ports| ports.contains(&fp)).await;

        if command == ADD_PORT && present {
            return (true, "Forwarded port already exists".into());
        }
        if command == REMOVE_PORT && !present {
            return (true, "Forwarded port doesn't exist".into());
        }

        let ipt = if fp.ip_version == IPV4 {
            IPTABLES
        } else {
            IP6TABLES
        };
        let action = if command == ADD_PORT { "-A" } else { "-D" };

        let to_dest = if fp.ip_version == IPV4 {
            format!("{}:{}", fp.toaddr, fp.toport)
        } else {
            format!("[{}]:{}", fp.toaddr, fp.toport)
        };

        let port_str = fp.port.to_string();
        let prerouting = command_output(
            ipt,
            &[
                "-t",
                "nat",
                action,
                "PREROUTING",
                "-p",
                fp.protocol.as_str(),
                "-i",
                WIFI_INTERFACE,
                "--dport",
                port_str.as_str(),
                "-j",
                "DNAT",
                "--to-destination",
                to_dest.as_str(),
            ],
        )
        .await;

        match prerouting {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                let msg = format!(
                    "Error in PREROUTING rule: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                error!("{}", msg);
                return (false, msg);
            }
            Err(e) => {
                return (false, e.to_string());
            }
        }

        let forward = command_output(
            ipt,
            &[
                action,
                "FORWARD",
                "-p",
                fp.protocol.as_str(),
                "-d",
                fp.toaddr.as_str(),
                "--dport",
                fp.toport.as_str(),
                "-m",
                "state",
                "--state",
                "NEW",
                "-j",
                "ACCEPT",
            ],
        )
        .await;

        match forward {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                let msg = format!(
                    "Error in FORWARD rule: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                error!("{}", msg);
                return (false, msg);
            }
            Err(e) => {
                return (false, e.to_string());
            }
        }

        let snapshot = with_ports(|ports| {
            if command == ADD_PORT {
                ports.push(fp);
            } else {
                ports.retain(|p| p != &fp);
            }
            serde_json::to_string(ports.as_slice()).ok()
        })
        .await;
        if let Some(data) = snapshot {
            let _ = tokio::fs::write(FORWARDED_PORTS_FILE, data).await;
        }
        (true, String::new())
    }
}
