//! Pure-safe packet parsing for the AF_PACKET capture loop.
//!
//! Takes a raw `&[u8]` frame (exactly what `recvfrom(2)` filled — including the
//! 14-byte Ethernet header) and extracts a typed [`FiveTuple`] (source/dest IP +
//! L4 protocol + source/dest port) plus the on-wire length the kernel reported.
//! There is NO `unsafe` here: every access is bounds-checked slicing over a
//! borrowed byte slice, mirroring the safe header-slicing style of the existing
//! `network::sources::parse_endpoint` parser. The audited `unsafe` socket seam
//! lives in [`crate`]'s `PacketSource`; this module is the safe consumer the
//! capture loop calls per packet.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// EtherType values that follow the 14-byte Ethernet (DIX) header.
const ETH_T_IPV4: u16 = 0x0800;
const ETH_T_IPV6: u16 = 0x86DD;
/// Ethernet header length: 6 dst + 6 src + 2 ethertype.
const ETH_HEADER_LEN: usize = 14;

/// A parsed connection identifier — the key the attribution layer joins against
/// `/proc/<pid>/net/{tcp,tcp6,udp,udp6}` socket entries. Only TCP/UDP over
/// IPv4/IPv6 produce a `FiveTuple`; everything else (ARP, ICMP, multicast noise
/// a classic cBPF filter would already have dropped in-kernel) yields `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FiveTuple {
    /// Source IP (IPv4 or IPv6).
    pub src: IpAddr,
    /// Destination IP (IPv4 or IPv6).
    pub dst: IpAddr,
    /// IP protocol number — `6` (TCP) or `17` (UDP); included so the join can
    /// distinguish a TCP socket from a UDP socket reusing the same ports.
    pub proto: u8,
    /// Source L4 port (TCP/UDP).
    pub src_port: u16,
    /// Destination L4 port (TCP/UDP).
    pub dst_port: u16,
}

/// Parse an Ethernet-framed packet buffer into a [`FiveTuple`].
///
/// `frame` is the bytes `recvfrom(2)` returned (Ethernet header included).
/// Returns `None` for anything that is not IPv4/IPv6 TCP/UDP, or a frame too
/// short to contain the required headers. Bounds are checked before every slice;
/// no `unsafe`, no panics on malformed input.
pub fn five_tuple(frame: &[u8]) -> Option<FiveTuple> {
    if frame.len() < ETH_HEADER_LEN {
        return None;
    }
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    let l3 = &frame[ETH_HEADER_LEN..];
    match ethertype {
        ETH_T_IPV4 => parse_ipv4(l3),
        ETH_T_IPV6 => parse_ipv6(l3),
        _ => None,
    }
}

fn parse_ipv4(l3: &[u8]) -> Option<FiveTuple> {
    // Minimum IPv4 header is 20 bytes; the IHL field (low nibble of byte 0) is
    // the header length in 32-bit words.
    if l3.len() < 20 {
        return None;
    }
    let version_ihl = l3[0];
    if version_ihl >> 4 != 4 {
        return None;
    }
    let ihl = (version_ihl & 0x0F) as usize;
    if ihl < 5 {
        return None;
    }
    let header_len = ihl * 4;
    if l3.len() < header_len {
        return None;
    }
    let proto = l3[9];
    let src = IpAddr::V4(Ipv4Addr::new(l3[12], l3[13], l3[14], l3[15]));
    let dst = IpAddr::V4(Ipv4Addr::new(l3[16], l3[17], l3[18], l3[19]));
    let l4 = &l3[header_len..];
    ports(proto, src, dst, l4)
}

fn parse_ipv6(l3: &[u8]) -> Option<FiveTuple> {
    // Fixed 40-byte IPv6 header; next-header field at byte 6. Extension headers
    // are NOT followed here (a cBPF filter keeps TCP/UDP only; uncommon chained
    // extension headers yield None and are simply not attributed — the safe
    // /proc connection path still accounts the socket they belong to).
    if l3.len() < 40 {
        return None;
    }
    if l3[0] >> 4 != 6 {
        return None;
    }
    let proto = l3[6];
    let mut src = [0u8; 16];
    src.copy_from_slice(&l3[8..24]);
    let mut dst = [0u8; 16];
    dst.copy_from_slice(&l3[24..40]);
    let src = IpAddr::V6(Ipv6Addr::from(src));
    let dst = IpAddr::V6(Ipv6Addr::from(dst));
    ports(proto, src, dst, &l3[40..])
}

/// Extract the TCP/UDP source/destination ports from an L4 slice for the given
/// IP protocol number. Returns `None` for non-TCP/UDP protocols.
fn ports(proto: u8, src: IpAddr, dst: IpAddr, l4: &[u8]) -> Option<FiveTuple> {
    // TCP (6) and UDP (17) both carry src+dest port as the first two u16s.
    const TCP: u8 = 6;
    const UDP: u8 = 17;
    if proto != TCP && proto != UDP {
        return None;
    }
    if l4.len() < 4 {
        return None;
    }
    let src_port = u16::from_be_bytes([l4[0], l4[1]]);
    let dst_port = u16::from_be_bytes([l4[2], l4[3]]);
    Some(FiveTuple {
        src,
        dst,
        proto,
        src_port,
        dst_port,
    })
}

#[cfg(test)]
#[path = "../tests/headless/parse.rs"]
mod tests;
