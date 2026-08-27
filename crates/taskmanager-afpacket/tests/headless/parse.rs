use super::*;

/// Build a minimal Ethernet+IPv4+TCP frame for a given 5-tuple.
fn ipv4_tcp_frame(src_ip: [u8; 4], dst_ip: [u8; 4], src_port: u16, dst_port: u16) -> Vec<u8> {
    let mut f = vec![0u8; 14 + 20 + 20];
    // ethertype IPv4
    f[12] = 0x08;
    f[13] = 0x00;
    // IPv4: version 4, IHL 5
    f[14] = 0x45;
    // proto TCP at byte 9 of the IPv4 header (frame offset 14+9 = 23)
    f[23] = 6;
    f[26..30].copy_from_slice(&src_ip);
    f[30..34].copy_from_slice(&dst_ip);
    // src/dst port at the TCP header (frame offset 14+20 = 34)
    f[34..36].copy_from_slice(&src_port.to_be_bytes());
    f[36..38].copy_from_slice(&dst_port.to_be_bytes());
    f
}

#[test]
fn parses_ipv4_tcp_five_tuple() {
    let frame = ipv4_tcp_frame([192, 168, 1, 5], [10, 0, 0, 1], 54321, 443);
    let t = five_tuple(&frame).expect("IPv4 TCP frame must parse");
    assert_eq!(t.src, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)));
    assert_eq!(t.dst, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
    assert_eq!(t.proto, 6);
    assert_eq!(t.src_port, 54321);
    assert_eq!(t.dst_port, 443);
}

#[test]
fn parses_ipv6_udp_five_tuple() {
    let mut f = vec![0u8; 14 + 40 + 8];
    f[12] = 0x86;
    f[13] = 0xDD;
    f[14] = 0x60; // version 6
    f[20] = 17; // next-header UDP at IPv6 offset 6 → frame 14+6=20
    f[22..38].copy_from_slice(&[1; 16]); // src
    f[38..54].copy_from_slice(&[2; 16]); // dst
    f[54..56].copy_from_slice(&1024u16.to_be_bytes()); // src port
    f[56..58].copy_from_slice(&53u16.to_be_bytes()); // dst port
    let t = five_tuple(&f).expect("IPv6 UDP frame must parse");
    assert_eq!(t.proto, 17);
    assert_eq!(t.src_port, 1024);
    assert_eq!(t.dst_port, 53);
    assert!(matches!(t.src, IpAddr::V6(_)));
    assert!(matches!(t.dst, IpAddr::V6(_)));
}

#[test]
fn rejects_non_ip_and_truncated_frames() {
    // ARP ethertype (0x0806) → None.
    let mut arp = vec![0u8; 14 + 28];
    arp[12] = 0x08;
    arp[13] = 0x06;
    assert_eq!(five_tuple(&arp), None);
    // Too short to even hold an Ethernet header.
    assert_eq!(five_tuple(&[0u8; 4]), None);
    // IPv4 frame truncated before the L4 ports.
    let mut short = vec![0u8; 14 + 20];
    short[12] = 0x08;
    short[13] = 0x00;
    short[14] = 0x45;
    short[23] = 6;
    assert_eq!(five_tuple(&short), None);
}

#[test]
fn rejects_icmp_and_bad_ihl() {
    let mut f = ipv4_tcp_frame([1, 2, 3, 4], [5, 6, 7, 8], 1, 2);
    f[23] = 1; // ICMP — not TCP/UDP
    assert_eq!(five_tuple(&f), None);
    // IHL = 0 is invalid (must be >= 5).
    let mut bad = ipv4_tcp_frame([1, 2, 3, 4], [5, 6, 7, 8], 1, 2);
    bad[14] = 0x40; // version 4, IHL 0
    assert_eq!(five_tuple(&bad), None);
}

#[test]
fn parses_ipv4_udp_five_tuple() {
    let mut f = ipv4_tcp_frame([10, 1, 2, 3], [8, 8, 8, 8], 5353, 53);
    f[23] = 17; // UDP
    let t = five_tuple(&f).expect("IPv4 UDP frame must parse");
    assert_eq!(t.proto, 17);
    assert_eq!(t.src_port, 5353);
    assert_eq!(t.dst_port, 53);
    assert_eq!(t.src, IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3)));
    assert_eq!(t.dst, IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)));
}

#[test]
fn parses_ipv6_tcp_five_tuple() {
    let mut f = vec![0u8; 14 + 40 + 20];
    f[12] = 0x86;
    f[13] = 0xDD;
    f[14] = 0x60; // version 6
    f[20] = 6; // next-header TCP
    f[22..38].copy_from_slice(&[0xfd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    f[38..54].copy_from_slice(&[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    f[54..56].copy_from_slice(&44300u16.to_be_bytes());
    f[56..58].copy_from_slice(&443u16.to_be_bytes());
    let t = five_tuple(&f).expect("IPv6 TCP frame must parse");
    assert_eq!(t.proto, 6);
    assert_eq!(t.src_port, 44300);
    assert_eq!(t.dst_port, 443);
    assert_eq!(
        t.src,
        IpAddr::V6("fd00::1".parse().expect("fixture address"))
    );
    assert_eq!(
        t.dst,
        IpAddr::V6("2001:db8::1".parse().expect("fixture address"))
    );
}

#[test]
fn honors_ipv4_options_header_length() {
    // IHL 6 = 24-byte IPv4 header; the L4 ports sit at frame offset
    // 14 + 24, not 14 + 20. A parser that ignores IHL would read the
    // option bytes as ports.
    let mut f = vec![0u8; 14 + 24 + 20];
    f[12] = 0x08;
    f[13] = 0x00;
    f[14] = 0x46; // version 4, IHL 6
    f[23] = 6; // proto TCP
    f[26..30].copy_from_slice(&[10, 0, 0, 1]);
    f[30..34].copy_from_slice(&[10, 0, 0, 2]);
    f[34..38].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // option bytes
    f[38..40].copy_from_slice(&1111u16.to_be_bytes()); // src port
    f[40..42].copy_from_slice(&2222u16.to_be_bytes()); // dst port
    let t = five_tuple(&f).expect("IPv4 frame with options must parse");
    assert_eq!(
        t.src_port, 1111,
        "options must be skipped, not read as ports"
    );
    assert_eq!(t.dst_port, 2222);
}

#[test]
fn rejects_wrong_ip_version_and_truncated_l4() {
    // Version 5 in the IPv4 header → None.
    let mut v5 = ipv4_tcp_frame([1, 2, 3, 4], [5, 6, 7, 8], 1, 2);
    v5[14] = 0x55;
    assert_eq!(five_tuple(&v5), None);
    // IPv4 header claims IHL 6 but the frame ends inside the options.
    let mut cut = ipv4_tcp_frame([1, 2, 3, 4], [5, 6, 7, 8], 1, 2);
    cut[14] = 0x46;
    cut.truncate(14 + 22);
    assert_eq!(five_tuple(&cut), None);
}
