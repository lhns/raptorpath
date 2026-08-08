//! Route and DNS management for the TUN tunnel.
//!
//! Adds/removes system routes so traffic flows through the tunnel,
//! and optionally configures DNS on the tunnel interface.

use std::net::IpAddr;
use tracing::info;

/// A route that was added and needs to be cleaned up on shutdown.
#[derive(Debug, Clone)]
pub struct ManagedRoute {
    pub destination: String, // CIDR, e.g. "192.168.50.0/24"
    pub gateway: IpAddr,     // TUN peer IP (gateway)
    pub iface: String,       // TUN interface name
}

/// DNS configuration that was applied and needs to be reverted.
#[derive(Debug, Clone)]
pub struct ManagedDns {
    pub server: IpAddr,
    pub iface: String,
    #[cfg(target_os = "linux")]
    pub previous_resolv_conf: Option<String>,
}

/// Add a route through the tunnel.
pub async fn add_route(route: &ManagedRoute) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        // route ADD destination MASK netmask gateway
        let (dest, mask) = parse_cidr_to_dest_mask(&route.destination)?;
        let output = tokio::process::Command::new("route")
            .args(["ADD", &dest, "MASK", &mask, &route.gateway.to_string()])
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!(
                "Failed to add route {}: {} {}",
                route.destination,
                stderr.trim(),
                stdout.trim()
            );
        }
        info!(dest = %route.destination, gw = %route.gateway, "added route");
    }

    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("ip")
            .args([
                "route",
                "add",
                &route.destination,
                "via",
                &route.gateway.to_string(),
                "dev",
                &route.iface,
            ])
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to add route {}: {}", route.destination, stderr.trim());
        }
        info!(dest = %route.destination, gw = %route.gateway, dev = %route.iface, "added route");
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = route;
        anyhow::bail!("Route management not supported on this platform");
    }

    Ok(())
}

/// Remove a previously added route.
pub async fn remove_route(route: &ManagedRoute) {
    #[cfg(target_os = "windows")]
    {
        if let Ok((dest, mask)) = parse_cidr_to_dest_mask(&route.destination) {
            let _ = tokio::process::Command::new("route")
                .args(["DELETE", &dest, "MASK", &mask])
                .output()
                .await;
            info!(dest = %route.destination, "removed route");
        }
    }

    #[cfg(target_os = "linux")]
    {
        let _ = tokio::process::Command::new("ip")
            .args(["route", "del", &route.destination, "dev", &route.iface])
            .output()
            .await;
        info!(dest = %route.destination, dev = %route.iface, "removed route");
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = route;
    }
}

/// Configure DNS on the tunnel interface.
pub async fn set_dns(dns: &mut ManagedDns) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let output = tokio::process::Command::new("netsh")
            .args([
                "interface",
                "ip",
                "set",
                "dns",
                &dns.iface,
                "static",
                &dns.server.to_string(),
            ])
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "Failed to set DNS on {}: {}. Make sure you are running as Administrator.",
                dns.iface,
                stderr.trim()
            );
        }
        info!(dns = %dns.server, iface = %dns.iface, "configured DNS on tunnel interface");
    }

    #[cfg(target_os = "linux")]
    {
        // Save current resolv.conf for restoration
        dns.previous_resolv_conf = std::fs::read_to_string("/etc/resolv.conf").ok();

        // Try resolvectl first (systemd-resolved)
        let resolvectl = tokio::process::Command::new("resolvectl")
            .args(["dns", &dns.iface, &dns.server.to_string()])
            .output()
            .await;

        match resolvectl {
            Ok(output) if output.status.success() => {
                // Also set this interface as default route for DNS
                let _ = tokio::process::Command::new("resolvectl")
                    .args(["domain", &dns.iface, "~."])
                    .output()
                    .await;
                info!(dns = %dns.server, iface = %dns.iface, "configured DNS via resolvectl");
            }
            _ => {
                // Fallback: write /etc/resolv.conf directly
                let content = format!(
                    "# Managed by raptorpath — previous config will be restored on shutdown\nnameserver {}\n",
                    dns.server
                );
                std::fs::write("/etc/resolv.conf", content)?;
                info!(dns = %dns.server, "configured DNS via /etc/resolv.conf");
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = dns;
        anyhow::bail!("DNS configuration not supported on this platform");
    }

    Ok(())
}

/// Revert DNS configuration.
pub async fn revert_dns(dns: &ManagedDns) {
    #[cfg(target_os = "windows")]
    {
        // Set DNS back to DHCP
        let _ = tokio::process::Command::new("netsh")
            .args([
                "interface",
                "ip",
                "set",
                "dns",
                &dns.iface,
                "dhcp",
            ])
            .output()
            .await;
        info!(iface = %dns.iface, "reverted DNS to DHCP");
    }

    #[cfg(target_os = "linux")]
    {
        // Try resolvectl revert first
        let resolvectl = tokio::process::Command::new("resolvectl")
            .args(["revert", &dns.iface])
            .output()
            .await;

        match resolvectl {
            Ok(output) if output.status.success() => {
                info!(iface = %dns.iface, "reverted DNS via resolvectl");
            }
            _ => {
                // Restore previous resolv.conf
                if let Some(ref prev) = dns.previous_resolv_conf {
                    let _ = std::fs::write("/etc/resolv.conf", prev);
                    info!("restored /etc/resolv.conf");
                }
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = dns;
    }
}

/// Parse CIDR notation into destination and netmask strings (for Windows `route` command).
#[cfg(target_os = "windows")]
fn parse_cidr_to_dest_mask(cidr: &str) -> anyhow::Result<(String, String)> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid CIDR: {cidr}");
    }
    let ip: std::net::IpAddr = parts[0].parse()?;
    let prefix: u8 = parts[1].parse()?;

    let mask = if prefix >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix)
    };

    // Network address (zero out host bits)
    let dest = match ip {
        std::net::IpAddr::V4(v4) => {
            let bits = u32::from(v4) & mask;
            std::net::Ipv4Addr::from(bits).to_string()
        }
        _ => anyhow::bail!("IPv6 routes not yet supported"),
    };

    let mask_str = std::net::Ipv4Addr::from(mask).to_string();
    Ok((dest, mask_str))
}

/// Compute the peer gateway IP from our TUN address.
/// For a /24 with address 10.99.0.1, the peer is assumed to be 10.99.0.2 (and vice versa).
pub fn infer_peer_ip(our_addr: IpAddr, prefix: u8) -> Option<IpAddr> {
    match our_addr {
        IpAddr::V4(v4) => {
            let bits = u32::from(v4);
            let host = bits & !mask_from_prefix(prefix);
            // If our host is .1, peer is .2; if .2, peer is .1
            let peer_host = if host == 1 { 2 } else { 1 };
            let net = bits & mask_from_prefix(prefix);
            Some(IpAddr::V4(std::net::Ipv4Addr::from(net | peer_host)))
        }
        _ => None,
    }
}

fn mask_from_prefix(prefix: u8) -> u32 {
    if prefix >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_peer_ip_from_dot_1() {
        let our = "10.99.0.1".parse().unwrap();
        let peer = infer_peer_ip(our, 24).unwrap();
        assert_eq!(peer, "10.99.0.2".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_infer_peer_ip_from_dot_2() {
        let our = "10.99.0.2".parse().unwrap();
        let peer = infer_peer_ip(our, 24).unwrap();
        assert_eq!(peer, "10.99.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_infer_peer_ip_other_host() {
        let our = "10.99.0.5".parse().unwrap();
        let peer = infer_peer_ip(our, 24).unwrap();
        assert_eq!(peer, "10.99.0.1".parse::<IpAddr>().unwrap());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_cidr_to_dest_mask() {
        let (dest, mask) = parse_cidr_to_dest_mask("192.168.50.0/24").unwrap();
        assert_eq!(dest, "192.168.50.0");
        assert_eq!(mask, "255.255.255.0");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_cidr_host_bits_zeroed() {
        let (dest, mask) = parse_cidr_to_dest_mask("192.168.50.100/24").unwrap();
        assert_eq!(dest, "192.168.50.0"); // host bits zeroed
        assert_eq!(mask, "255.255.255.0");
    }
}
