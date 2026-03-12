//! Tests for route management and DNS configuration.

use raptorpath::routing;
use std::net::IpAddr;

#[test]
fn test_infer_peer_ip_server_side() {
    // Server is .1, peer should be .2
    let our: IpAddr = "10.99.0.1".parse().unwrap();
    let peer = routing::infer_peer_ip(our, 24).unwrap();
    assert_eq!(peer, "10.99.0.2".parse::<IpAddr>().unwrap());
}

#[test]
fn test_infer_peer_ip_client_side() {
    // Client is .2, peer should be .1
    let our: IpAddr = "10.99.0.2".parse().unwrap();
    let peer = routing::infer_peer_ip(our, 24).unwrap();
    assert_eq!(peer, "10.99.0.1".parse::<IpAddr>().unwrap());
}

#[test]
fn test_infer_peer_ip_arbitrary_host() {
    // Any other host (.5) should map to .1
    let our: IpAddr = "10.99.0.5".parse().unwrap();
    let peer = routing::infer_peer_ip(our, 24).unwrap();
    assert_eq!(peer, "10.99.0.1".parse::<IpAddr>().unwrap());
}

#[test]
fn test_infer_peer_ip_different_subnet() {
    let our: IpAddr = "172.16.0.1".parse().unwrap();
    let peer = routing::infer_peer_ip(our, 30).unwrap();
    assert_eq!(peer, "172.16.0.2".parse::<IpAddr>().unwrap());
}

#[test]
fn test_infer_peer_ip_16_prefix() {
    let our: IpAddr = "10.0.0.1".parse().unwrap();
    let peer = routing::infer_peer_ip(our, 16).unwrap();
    assert_eq!(peer, "10.0.0.2".parse::<IpAddr>().unwrap());
}

#[test]
fn test_config_resolve_with_routes_and_dns() {
    use raptorpath::config::{self, RaptorpathConfig};

    let cfg = RaptorpathConfig {
        route: Some(vec!["192.168.50.0/24".into(), "10.0.0.0/8".into()]),
        dns: Some("10.99.0.1".into()),
        ..Default::default()
    };
    let (peer_config, _) = config::resolve(&cfg).unwrap();
    assert_eq!(peer_config.routes.len(), 2);
    assert_eq!(peer_config.routes[0], "192.168.50.0/24");
    assert_eq!(peer_config.dns.unwrap(), "10.99.0.1".parse::<IpAddr>().unwrap());
}

#[test]
fn test_config_resolve_no_routes_default() {
    use raptorpath::config::{self, RaptorpathConfig};

    let cfg = RaptorpathConfig::default();
    let (peer_config, _) = config::resolve(&cfg).unwrap();
    assert!(peer_config.routes.is_empty());
    assert!(peer_config.dns.is_none());
}

#[test]
fn test_config_invalid_dns() {
    use raptorpath::config::{self, RaptorpathConfig};

    let cfg = RaptorpathConfig {
        dns: Some("not-an-ip".into()),
        ..Default::default()
    };
    assert!(config::resolve(&cfg).is_err());
}

#[test]
fn test_config_toml_with_routes_and_dns() {
    use raptorpath::config::RaptorpathConfig;

    let toml_str = r#"
        route = ["192.168.50.0/24", "10.0.0.0/8"]
        dns = "8.8.8.8"
    "#;
    let cfg: RaptorpathConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.route.as_ref().unwrap().len(), 2);
    assert_eq!(cfg.dns.as_deref(), Some("8.8.8.8"));
}

#[test]
fn test_managed_route_debug() {
    let route = routing::ManagedRoute {
        destination: "192.168.50.0/24".into(),
        gateway: "10.99.0.2".parse().unwrap(),
        iface: "rpath0".into(),
    };
    let debug = format!("{:?}", route);
    assert!(debug.contains("192.168.50.0/24"));
    assert!(debug.contains("10.99.0.2"));
}
