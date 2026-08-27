use super::*;

#[test]
fn loopback_filter_detects_macos_aliases() {
    assert!(is_loopback("lo0"));
    assert!(is_loopback("loopback"));
    assert!(!is_loopback("en0"));
    assert!(!is_loopback("utun3"));
}

#[test]
fn parse_media_speed_handles_common_macos_ethernet_tokens() {
    assert_eq!(parse_media_speed("1000baseT"), Some(1000));
    assert_eq!(parse_media_speed("100baseTX"), Some(100));
    assert_eq!(parse_media_speed("10baseT"), Some(10));
    // Multi-gig forms use the G-suffix convention.
    assert_eq!(parse_media_speed("2.5GBASE-T"), Some(2500));
    assert_eq!(parse_media_speed("5GBASE-T"), Some(5000));
    assert_eq!(parse_media_speed("10GBASE-T"), Some(10000));
}

#[test]
fn parse_media_speed_returns_none_for_unrecognised_forms() {
    // Wireless identifiers, missing units, and bare words never fabricate a
    // number — the link speed scalar degrades honestly instead.
    assert_eq!(parse_media_speed("IEEE802.11"), None);
    assert_eq!(parse_media_speed("autoselect"), None);
    assert_eq!(parse_media_speed("none"), None);
    assert_eq!(parse_media_speed("<full-duplex>"), None);
}

#[test]
fn parse_ifconfig_a_extracts_speed_and_carrier_per_interface() {
    // Verbatim-shaped `ifconfig -a` excerpt: two Ethernet interfaces, one
    // up with a negotiated 1 Gbps link, one down with no media.
    let stdout = "\
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
\toptions=400<CHANNEL_IO>
\tether aa:bb:cc:dd:ee:ff
\tinet6 fe80::1%en0 prefixlen 64 scopeid 0x4
\tinet 192.168.1.10 netmask 0xffffff00 broadcast 192.168.1.255
\tmedia: autoselect (1000baseT <full-duplex>)
\tstatus: active

en4: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
\toptions=400<CHANNEL_IO>
\tether 11:22:33:44:55:66
\tmedia: none
\tstatus: inactive
";
    let map = parse_ifconfig_a(stdout);
    assert_eq!(
        map.get("en0"),
        Some(&IfaceLink {
            speed_mbps: Some(1000),
            up: true,
        }),
    );
    assert_eq!(
        map.get("en4"),
        Some(&IfaceLink {
            speed_mbps: None,
            up: false,
        }),
    );
}

#[test]
fn parse_ifconfig_a_is_empty_when_tool_is_absent_or_unparsable() {
    // On a Linux CI host `ifconfig` may be missing entirely; the runner
    // returns an empty stdout, so no link facts are fabricated.
    assert!(parse_ifconfig_a("").is_empty());
    assert!(parse_ifconfig_a("ifconfig: command not found\n").is_empty());
}

#[test]
fn parse_airport_network_reads_ssid_when_associated() {
    let stdout = "Current Wi-Fi Network: HomeStudio-5G\n";
    assert_eq!(
        parse_airport_network(stdout),
        WifiSsidState::Associated("HomeStudio-5G".to_string()),
    );
}

#[test]
fn parse_airport_network_reports_not_associated_without_an_ssid() {
    // Empty SSID after the prefix, or no current-network line at all.
    assert_eq!(
        parse_airport_network("Current Wi-Fi Network: \n"),
        WifiSsidState::NotAssociated,
    );
    assert_eq!(parse_airport_network(""), WifiSsidState::NotAssociated);
}

#[test]
fn parse_wifi_hardware_port_locates_the_wifi_device_name() {
    let stdout = "\
Hardware Port: Wi-Fi
Device: en0
Wi-Fi ID: 41D88B7E-xxxx-xxxx-xxxx

Hardware Port: Thunderbolt 1
Device: en5

Hardware Port: Ethernet Adaptor (USB 3.0)
Device: en6
";
    assert_eq!(parse_wifi_hardware_port(stdout), Some("en0".to_string()));
}

#[test]
fn parse_wifi_hardware_port_returns_none_when_no_wifi_port_present() {
    // A desktop Mac without Wi-Fi hardware lists no Wi-Fi port.
    let stdout = "\
Hardware Port: Ethernet Adaptor (USB 3.0)
Device: en0
";
    assert_eq!(parse_wifi_hardware_port(stdout), None);
    assert_eq!(parse_wifi_hardware_port(""), None);
}
