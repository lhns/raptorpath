//! ADR-0012: Configuration and profile tests.

use raptorpath::config::{self, Profile, RaptorpathConfig};

#[test]
fn test_profile_home_defaults() {
    let cfg = Profile::Home.defaults();
    assert_eq!(cfg.target_tail_loss, Some(1e-4));
    assert_eq!(cfg.max_fec_overhead, Some(0.3));
    assert_eq!(cfg.protocol_hint.as_deref(), Some("auto"));
}

#[test]
fn test_profile_datacenter_defaults() {
    let cfg = Profile::Datacenter.defaults();
    assert_eq!(cfg.target_tail_loss, Some(1e-6));
    assert_eq!(cfg.max_fec_overhead, Some(0.5));
    assert_eq!(cfg.protocol_hint.as_deref(), Some("bulk"));
}

#[test]
fn test_merge_cli_overrides_toml() {
    let toml_config = RaptorpathConfig {
        tun_name: Some("from_toml".into()),
        target_tail_loss: Some(1e-5),
        max_fec_overhead: Some(0.3),
        ..Default::default()
    };
    let cli_config = RaptorpathConfig {
        target_tail_loss: Some(1e-4), // CLI overrides
        ..Default::default()
    };
    let merged = config::merge(toml_config, cli_config);

    assert_eq!(merged.tun_name.as_deref(), Some("from_toml")); // from toml
    assert_eq!(merged.target_tail_loss, Some(1e-4)); // CLI wins
    assert_eq!(merged.max_fec_overhead, Some(0.3)); // from toml
}

#[test]
fn test_merge_profile_then_cli() {
    let profile = Profile::Home.defaults();
    let cli = RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["0.0.0.0:4433".into()]),
        ..Default::default()
    };
    let merged = config::merge(profile, cli);

    assert_eq!(merged.server, Some(true)); // from CLI
    assert_eq!(merged.target_tail_loss, Some(1e-4)); // from profile
    assert!(merged.bind.is_some()); // from CLI
}

#[test]
fn test_resolve_with_defaults() {
    let cfg = RaptorpathConfig::default();
    let (peer, status) = config::resolve(&cfg).unwrap();

    assert_eq!(peer.tun_name, "rpath0");
    assert_eq!(peer.tun_addr, "10.99.0.1/24");
    assert!(!peer.is_server);
    assert!(peer.bind_addrs.is_empty());
    assert!(status.is_none());
}

#[test]
fn test_resolve_with_status_addr() {
    let cfg = RaptorpathConfig {
        status_addr: Some("127.0.0.1:9820".into()),
        ..Default::default()
    };
    let (_, status) = config::resolve(&cfg).unwrap();
    assert_eq!(
        status.unwrap().to_string(),
        "127.0.0.1:9820"
    );
}

#[test]
fn test_resolve_invalid_bind_addr() {
    let cfg = RaptorpathConfig {
        bind: Some(vec!["not_an_addr".into()]),
        ..Default::default()
    };
    assert!(config::resolve(&cfg).is_err());
}

#[test]
fn test_resolve_invalid_status_addr() {
    let cfg = RaptorpathConfig {
        status_addr: Some("not_valid".into()),
        ..Default::default()
    };
    assert!(config::resolve(&cfg).is_err());
}

#[test]
fn test_toml_parse_minimal() {
    let toml_str = r#"
        server = true
        tun_name = "test0"
    "#;
    let cfg: RaptorpathConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.server, Some(true));
    assert_eq!(cfg.tun_name.as_deref(), Some("test0"));
    assert!(cfg.bind.is_none()); // not specified
}

#[test]
fn test_toml_parse_full() {
    let toml_str = r#"
        server = false
        bind = ["0.0.0.0:4433", "0.0.0.0:4434"]
        peer = ["1.2.3.4:4433", "1.2.3.4:4434"]
        tun_name = "rpath0"
        tun_addr = "10.99.0.1/24"
        target_tail_loss = 1e-6
        max_fec_overhead = 0.5
        protocol_hint = "bulk"
        status_addr = "127.0.0.1:9820"
    "#;
    let cfg: RaptorpathConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.bind.as_ref().unwrap().len(), 2);
    assert_eq!(cfg.peer.as_ref().unwrap().len(), 2);
    assert_eq!(cfg.target_tail_loss, Some(1e-6));
    assert_eq!(cfg.protocol_hint.as_deref(), Some("bulk"));
    assert_eq!(cfg.status_addr.as_deref(), Some("127.0.0.1:9820"));
}

#[test]
fn test_profile_parse() {
    assert!("home".parse::<Profile>().is_ok());
    assert!("datacenter".parse::<Profile>().is_ok());
    assert!("dc".parse::<Profile>().is_ok());
    assert!("HOME".parse::<Profile>().is_ok()); // case insensitive
    assert!("unknown".parse::<Profile>().is_err());
}

#[test]
fn test_three_layer_merge() {
    // Profile → TOML → CLI
    let profile = Profile::Home.defaults();
    let toml_cfg = RaptorpathConfig {
        tun_name: Some("custom".into()),
        ..Default::default()
    };
    let cli = RaptorpathConfig {
        server: Some(true),
        ..Default::default()
    };

    let merged = config::merge(config::merge(profile, toml_cfg), cli);
    assert_eq!(merged.server, Some(true));        // CLI
    assert_eq!(merged.tun_name.as_deref(), Some("custom")); // TOML
    assert_eq!(merged.target_tail_loss, Some(1e-4));  // Profile
}
