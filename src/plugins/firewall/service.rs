//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//
//! iptables-based firewall port forwarding service

use crate::utils::command_output;
use parking_lot::Mutex;
use std::sync::LazyLock;
use serde::{Deserialize, Serialize};
use tracing::error;

const IPTABLES: &str = "/usr/sbin/iptables";
const IP6TABLES: &str = "/usr/sbin/ip6tables";
pub const FORWARDED_PORTS_FILE: &str = "/tmp/summit-rcm.ports";
pub const ADD_PORT: &str = "addForwardPort";
pub const REMOVE_PORT: &str = "removeForwardPort";
pub const PORT_COMMANDS: &[&str] = &[ADD_PORT, REMOVE_PORT];
const WIFI_INTERFACE: &str = "wlan0";
pub const IPV4: &str = "ipv4";
pub const IPV6: &str = "ipv6";
pub const IP_VERSIONS: &[&str] = &[IPV4, IPV6];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "api-docs", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ForwardedPort {
    pub port: u16,
    pub protocol: String,
    pub toport: String,
    pub toaddr: String,
    pub ip_version: String,
}

static FORWARDED_PORTS: LazyLock<Mutex<Vec<ForwardedPort>>> = LazyLock::new(|| {
    Mutex::new(load_ports())
});

fn load_ports() -> Vec<ForwardedPort> {
    if let Ok(data) = std::fs::read_to_string(FORWARDED_PORTS_FILE) {
        if let Ok(v) = serde_json::from_str::<Vec<ForwardedPort>>(&data) {
            return v;
        }
    }
    Vec::new()
}

fn save_ports(ports: &[ForwardedPort]) {
    if let Ok(data) = serde_json::to_string(ports) {
        let _ = std::fs::write(FORWARDED_PORTS_FILE, data);
    }
}

pub struct FirewallService;

impl FirewallService {
    pub fn get_forwarded_ports() -> Vec<ForwardedPort> {
        FORWARDED_PORTS.lock().clone()
    }

    pub fn port_is_present(port: &ForwardedPort) -> bool {
        FORWARDED_PORTS.lock().contains(port)
    }

    pub async fn configure_forwarded_port(command: &str, fp: ForwardedPort) -> (bool, String) {
        let present = Self::port_is_present(&fp);

        if command == ADD_PORT && present {
            return (true, "Forwarded port already exists".into());
        }
        if command == REMOVE_PORT && !present {
            return (true, "Forwarded port doesn't exist".into());
        }

        let ipt = if fp.ip_version == IPV4 { IPTABLES } else { IP6TABLES };
        let action = if command == ADD_PORT { "-A" } else { "-D" };

        let to_dest = if fp.ip_version == IPV4 {
            format!("{}:{}", fp.toaddr, fp.toport)
        } else {
            format!("[{}]:{}", fp.toaddr, fp.toport)
        };

                let port_str = fp.port.to_string();
                let prerouting = command_output(
                    ipt,
                        &["-t", "nat", action, "PREROUTING", "-p", fp.protocol.as_str(),
                            "-i", WIFI_INTERFACE, "--dport", port_str.as_str(),
                            "-j", "DNAT", "--to-destination", to_dest.as_str()],
                ).await;

        match prerouting {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                let msg = format!("Error in PREROUTING rule: {}", String::from_utf8_lossy(&out.stderr));
                error!("{}", msg);
                return (false, msg);
            }
            Err(e) => { return (false, e.to_string()); }
        }

                let forward = command_output(
                    ipt,
                        &[action, "FORWARD", "-p", fp.protocol.as_str(), "-d", fp.toaddr.as_str(),
                            "--dport", fp.toport.as_str(), "-m", "state", "--state", "NEW", "-j", "ACCEPT"],
                ).await;

        match forward {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                let msg = format!("Error in FORWARD rule: {}", String::from_utf8_lossy(&out.stderr));
                error!("{}", msg);
                return (false, msg);
            }
            Err(e) => { return (false, e.to_string()); }
        }

        let mut ports = FORWARDED_PORTS.lock();
        if command == ADD_PORT {
            ports.push(fp);
        } else {
            ports.retain(|p| p != &fp);
        }
        save_ports(&ports);
        (true, String::new())
    }

}
