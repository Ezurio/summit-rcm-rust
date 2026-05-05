//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

use serde_json::Value;

use super::super::NetworkManagerService;

impl NetworkManagerService {
    pub(crate) fn to_camel_case(value: &str) -> String {
        if value
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        {
            return value.to_string();
        }

        let mut words = Vec::new();
        let mut current = String::new();
        let mut previous_was_lower_or_digit = false;

        for character in value.chars() {
            if character == '_' || character == '-' {
                if !current.is_empty() {
                    words.push(current);
                    current = String::new();
                }
                previous_was_lower_or_digit = false;
                continue;
            }

            let is_upper = character.is_ascii_uppercase();
            if is_upper && previous_was_lower_or_digit && !current.is_empty() {
                words.push(current);
                current = String::new();
            }
            previous_was_lower_or_digit = character.is_ascii_lowercase() || character.is_ascii_digit();
            current.push(character);
        }

        if !current.is_empty() {
            words.push(current);
        }

        if words.is_empty() {
            return value.to_string();
        }

        let mut result = String::new();
        for (index, word) in words.into_iter().enumerate() {
            let lowercase = word.to_ascii_lowercase();
            if index == 0 {
                result.push_str(&lowercase);
            } else {
                let mut chars = lowercase.chars();
                if let Some(first) = chars.next() {
                    result.push(first.to_ascii_uppercase());
                    result.extend(chars);
                }
            }
        }

        result
    }

    fn convert_nm_property_name(property_name: &str, is_legacy: bool) -> String {
        let mapped = match property_name {
            "Ssid" => Some(("Ssid", "ssid")),
            "HwAddress" => Some(("HwAddress", "hwAddress")),
            "MaxBitrate" => Some(("Maxbitrate", "maxBitrate")),
            "Flags" => Some(("Flags", "flags")),
            "FlagsList" => Some(("FlagsList", "flagsList")),
            "WpaFlags" => Some(("Wpaflags", "wpaFlags")),
            "WpaFlagsList" => Some(("WpaFlagsList", "wpaFlagsList")),
            "RsnFlags" => Some(("Rsnflags", "rsnFlags")),
            "RsnFlagsList" => Some(("RsnFlagsList", "rsnFlagsList")),
            "Bandwidth" => Some(("Bandwidth", "bandwidth")),
            "Strength" => Some(("Strength", "strength")),
            "Frequency" => Some(("Frequency", "frequency")),
            "Signal" => Some(("Signal", "signal")),
            "Channel" => Some(("Channel", "channel")),
            "State" => Some(("State", "state")),
            "StateText" => Some(("StateText", "stateText")),
            "Mtu" => Some(("Mtu", "mtu")),
            "DeviceType" => Some(("DeviceType", "deviceType")),
            "DeviceTypeText" => Some(("DeviceTypeText", "deviceTypeText")),
            "Addresses" => Some(("Addresses", "addresses")),
            "AddressData" => Some(("AddressData", "addressData")),
            "next-hop" => Some(("next_hop", "nextHop")),
            "Routes" => Some(("Routes", "routes")),
            "RouteData" => Some(("RouteData", "routeData")),
            "Gateway" => Some(("Gateway", "gateway")),
            "Domains" => Some(("Domains", "domains")),
            "NameserverData" => Some(("NameserverData", "nameservers")),
            "WinsServerData" => Some(("WinsServerData", "winsServers")),
            "Options" => Some(("Options", "options")),
            "PermHwAddress" => Some(("PermHwAddress", "permHwAddress")),
            "Speed" => Some(("Speed", "speed")),
            "Carrier" => Some(("Carrier", "carrier")),
            "Bitrate" => Some(("Bitrate", "bitrate")),
            "Mode" => Some(("Mode", "mode")),
            "RegDomain" => Some(("RegDomain", "regDomain")),
            "LastScan" => Some(("LastScan", "lastScan")),
            "id" => Some(("id", "id")),
            "interface-name" => Some(("interface-name", "interface-name")),
            "permissions" => Some(("permissions", "permissions")),
            "type" => Some(("type", "type")),
            "uuid" => Some(("uuid", "uuid")),
            "zone" => Some(("zone", "zone")),
            "connection_active" => Some(("connection_active", "activeConnection")),
            "Ip4Config" => Some(("ip4config", "ip4Config")),
            "Ip6Config" => Some(("ip6config", "ip6Config")),
            "Dhcp4Config" => Some(("dhcp4config", "dhcp4Config")),
            "Dhcp6Config" => Some(("dhcp6config", "dhcp6Config")),
            "ActiveAccessPoint" => Some(("activeaccesspoint", "activeAccessPoint")),
            "available_connections" => Some(("available_connections", "availableConnections")),
            "IpInterface" => Some(("ip_interface", "ipInterface")),
            "DriverVersion" => Some(("driver_version", "driverVersion")),
            "FirmwareVersion" => Some(("firmware_version", "firmwareVersion")),
            "StateReason" => Some(("state_reason", "stateReason")),
            "FirmwareMissing" => Some(("firmware_missing", "firmwareMissing")),
            "NmPluginMissing" => Some(("nm_plugin_missing", "nmPluginMissing")),
            "PhysicalPortId" => Some(("physical_port_id", "physicalPortId")),
            "MeteredText" => Some(("metered_text", "meteredText")),
            "LldpNeighbors" => Some(("lldp_neighbors", "lldpNeighbors")),
            "Ip4Connectivity" => Some(("ip4connectivity", "ip4Connectivity")),
            "Ip4ConnectivityText" => Some(("ip4connectivity_text", "ip4ConnectivityText")),
            "Ip6Connectivity" => Some(("ip6connectivity", "ip6Connectivity")),
            "Ip6ConnectivityText" => Some(("ip6connectivity_text", "ip6ConnectivityText")),
            "InterfaceFlags" => Some(("interface_flags", "interfaceFlags")),
            "subnet_mask" => Some(("subnet_mask", "subnetMask")),
            "time_offset" => Some(("time_offset", "timeOffset")),
            "domain_name_servers" => Some(("domain_name_servers", "domainNameServers")),
            "host_name" => Some(("host_name", "hostName")),
            "domain_name" => Some(("domain_name", "domainName")),
            "interface_mtu" => Some(("interface_mtu", "interfaceMtu")),
            "broadcast_address" => Some(("broadcast_address", "broadcastAddress")),
            "rfc3442_classless_static_routes" => Some(("rfc3442_classless_static_routes", "rfc3442ClasslessStaticRoutes")),
            "routers" => Some(("routers", "routers")),
            "static_routes" => Some(("static_routes", "staticRoutes")),
            "nis_domain" => Some(("nis_domain", "nisDomain")),
            "nis_servers" => Some(("nis_servers", "nisServers")),
            "ntp_servers" => Some(("ntp_servers", "ntpServers")),
            "dhcp_server_identifier" => Some(("dhcp_server_identifier", "dhcpServerIdentifier")),
            "domain_search" => Some(("domain_search", "domainSearch")),
            "ms_classless_static_routes" => Some(("ms_classless_static_routes", "msClasslessStaticRoutes")),
            "requested_rfc3442_classless_static_routes" => Some(("requested_rfc3442_classless_static_routes", "requestedrfc3442Classlessstaticroutes")),
            "wpad" => Some(("wpad", "wpad")),
            "root_path" => Some(("root_path", "rootPath")),
            "time_servers" => Some(("time_servers", "timeServers")),
            "ien116_name_servers" => Some(("ien116_name_servers", "ien116NameServers")),
            "log_servers" => Some(("log_servers", "logServers")),
            "cookie_servers" => Some(("cookie_servers", "cookieServers")),
            "lpr_servers" => Some(("lpr_servers", "lprServers")),
            "impress_servers" => Some(("impress_servers", "impressServers")),
            "resource_location_servers" => Some(("resource_location_servers", "resourceLocationServers")),
            "boot_size" => Some(("boot_size", "bootSize")),
            "merit_dump" => Some(("merit_dump", "meritDump")),
            "swap_server" => Some(("swap_server", "swapServer")),
            "extensions_path" => Some(("extensions_path", "extensionsPath")),
            "ip_forwarding" => Some(("ip_forwarding", "ipForwarding")),
            "non_local_source_routing" => Some(("non_local_source_routing", "nonLocalSourceRouting")),
            "policy_filter" => Some(("policy_filter", "policyFilter")),
            "max_dgram_reassembly" => Some(("max_dgram_reassembly", "maxDgramReassembly")),
            "default_ip_ttl" => Some(("default_ip_ttl", "defaultIpTtl")),
            "path_mtu_aging_timeout" => Some(("path_mtu_aging_timeout", "pathMtuAgingTimeout")),
            "path_mtu_plateau_table" => Some(("path_mtu_plateau_table", "pathMtuPlateauTable")),
            "all_subnets_local" => Some(("all_subnets_local", "allSubnetsLocal")),
            "perform_mask_discovery" => Some(("perform_mask_discovery", "performMaskDiscovery")),
            "mask_supplier" => Some(("mask_supplier", "maskSupplier")),
            "router_discovery" => Some(("router_discovery", "routerDiscovery")),
            "router_solicitation_address" => Some(("router_solicitation_address", "routerSolicitationAddress")),
            "trailer_encapsulation" => Some(("trailer_encapsulation", "trailerEncapsulation")),
            "arp_cache_timeout" => Some(("arp_cache_timeout", "arpCacheTimeout")),
            "ieee802_3_encapsulation" => Some(("ieee802_3_encapsulation", "ieee802_3_encapsulation")),
            "default_tcp_ttl" => Some(("default_tcp_ttl", "defaultTcpTtl")),
            "tcp_keepalive_internal" => Some(("tcp_keepalive_internal", "tcpKeepaliveInternal")),
            "tcp_keepalive_garbage" => Some(("tcp_keepalive_garbage", "tcpKeepaliveGarbage")),
            "vendor_encapsulated_options" => Some(("vendor_encapsulated_options", "vendorEncapsulatedOptions")),
            "netbios_name_servers" => Some(("netbios_name_servers", "netbiosNameServers")),
            "netbios_dd_server" => Some(("netbios_dd_server", "netbiosDdServer")),
            "font_servers" => Some(("font_servers", "fontServers")),
            "x_display_manager" => Some(("x_display_manager", "xDisplayManager")),
            "dhcp_lease_time" => Some(("dhcp_lease_time", "dhcpLeaseTime")),
            "dhcp_renewal_time" => Some(("dhcp_renewal_time", "dhcpRenewalTime")),
            "dhcp_rebinding_time" => Some(("dhcp_rebinding_time", "dhcpRebindingTime")),
            "dhcp_client_identifier" => Some(("dhcp_client_identifier", "dhcpClientIdentifier")),
            "dhcp6_client_id" => Some(("dhcp6_client_id", "dhcp6Clientid")),
            "dhcp6_name_servers" => Some(("dhcp6_name_servers", "dhcp6Nameservers")),
            _ => None,
        };

        if let Some((legacy, non_legacy)) = mapped {
            return if is_legacy {
                legacy.to_string()
            } else {
                non_legacy.to_string()
            };
        }

        Self::to_camel_case(property_name)
    }

    pub(crate) fn convert_property_names(value: Value, is_legacy: bool) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, value)| {
                        (
                            Self::convert_nm_property_name(&key, is_legacy),
                            Self::convert_property_names(value, is_legacy),
                        )
                    })
                    .collect(),
            ),
            Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(|value| Self::convert_property_names(value, is_legacy))
                    .collect(),
            ),
            value => value,
        }
    }
}
