//! Bounded process network connection enumeration via Win32 IP Helper.

use std::net::SocketAddr;
#[cfg(windows)]
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

use crate::WindowsApiError;

/// Maximum table size permitted for memory allocation (16 MiB).
const MAX_TABLE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum entries parsed per table to prevent CPU exhaustion.
const MAX_ENTRIES: usize = 65_536;

/// Protocol transport for process socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsTransportProtocol {
    Tcp,
    Udp,
}

/// TCP connection state decoded from Win32 MIB_TCP_STATE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsTcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
    DeleteTcb,
    Unknown,
}

impl WindowsTcpState {
    pub const fn from_mib(state: u32) -> Self {
        match state {
            1 => Self::Closed,
            2 => Self::Listen,
            3 => Self::SynSent,
            4 => Self::SynReceived,
            5 => Self::Established,
            6 => Self::FinWait1,
            7 => Self::FinWait2,
            8 => Self::CloseWait,
            9 => Self::Closing,
            10 => Self::LastAck,
            11 => Self::TimeWait,
            12 => Self::DeleteTcb,
            _ => Self::Unknown,
        }
    }
}

/// A decoded process network connection entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsProcessConnection {
    pub pid: u32,
    pub protocol: WindowsTransportProtocol,
    pub local_addr: SocketAddr,
    pub remote_addr: Option<SocketAddr>,
    pub state: WindowsTcpState,
}

/// Query all active TCP and UDP connections with owning process IDs.
#[must_use = "inspect process network connection query result"]
pub fn query_process_network_connections() -> Result<Vec<WindowsProcessConnection>, WindowsApiError>
{
    #[cfg(windows)]
    {
        query_process_network_connections_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_process_network_connections_windows()
-> Result<Vec<WindowsProcessConnection>, WindowsApiError> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetExtendedUdpTable, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
    };
    use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

    let mut connections = Vec::new();

    // 1. TCP IPv4
    parse_extended_table(
        |ptr, size| {
            // SAFETY: `GetExtendedTcpTable` writes to out-buffer according to size.
            unsafe {
                GetExtendedTcpTable(
                    ptr,
                    size,
                    false,
                    AF_INET.0.into(),
                    TCP_TABLE_OWNER_PID_ALL,
                    0,
                )
            }
        },
        &mut connections,
        parse_tcp4_table,
    )?;

    // 2. TCP IPv6
    parse_extended_table(
        |ptr, size| {
            // SAFETY: `GetExtendedTcpTable` writes to out-buffer according to size.
            unsafe {
                GetExtendedTcpTable(
                    ptr,
                    size,
                    false,
                    AF_INET6.0.into(),
                    TCP_TABLE_OWNER_PID_ALL,
                    0,
                )
            }
        },
        &mut connections,
        parse_tcp6_table,
    )?;

    // 3. UDP IPv4
    parse_extended_table(
        |ptr, size| {
            // SAFETY: `GetExtendedUdpTable` writes to out-buffer according to size.
            unsafe {
                GetExtendedUdpTable(ptr, size, false, AF_INET.0.into(), UDP_TABLE_OWNER_PID, 0)
            }
        },
        &mut connections,
        parse_udp4_table,
    )?;

    // 4. UDP IPv6
    parse_extended_table(
        |ptr, size| {
            // SAFETY: `GetExtendedUdpTable` writes to out-buffer according to size.
            unsafe {
                GetExtendedUdpTable(ptr, size, false, AF_INET6.0.into(), UDP_TABLE_OWNER_PID, 0)
            }
        },
        &mut connections,
        parse_udp6_table,
    )?;

    Ok(connections)
}

#[cfg(windows)]
fn parse_extended_table<F, P>(
    mut call: F,
    out: &mut Vec<WindowsProcessConnection>,
    parser: P,
) -> Result<(), WindowsApiError>
where
    F: FnMut(Option<*mut core::ffi::c_void>, &mut u32) -> u32,
    P: Fn(&[u8], &mut Vec<WindowsProcessConnection>) -> Result<(), WindowsApiError>,
{
    let mut size: u32 = 0;
    let _ = call(None, &mut size);
    if size == 0 {
        return Ok(());
    }
    let size_usize = usize::try_from(size).map_err(|_| WindowsApiError::ResourceLimit)?;
    if size_usize > MAX_TABLE_BYTES {
        return Err(WindowsApiError::ResourceLimit);
    }
    let mut buffer = vec![0u8; size_usize];
    let status = call(Some(buffer.as_mut_ptr().cast()), &mut size);
    if status != 0 {
        return Ok(());
    }
    parser(&buffer, out)
}

#[cfg(windows)]
fn parse_tcp4_table(
    buf: &[u8],
    out: &mut Vec<WindowsProcessConnection>,
) -> Result<(), WindowsApiError> {
    if buf.len() < 4 {
        return Ok(());
    }
    let num_entries = u32::from_ne_bytes(buf[0..4].try_into().unwrap_or([0; 4])) as usize;
    let entries = num_entries.min(MAX_ENTRIES);
    let row_size = 24; // dwState(4) + dwLocalAddr(4) + dwLocalPort(4) + dwRemoteAddr(4) + dwRemotePort(4) + dwOwningPid(4)
    if buf.len() < 4 + entries * row_size {
        return Ok(());
    }
    for i in 0..entries {
        let offset = 4 + i * row_size;
        let row = &buf[offset..offset + row_size];
        let state_raw = u32::from_ne_bytes(row[0..4].try_into().unwrap_or([0; 4]));
        let local_addr = Ipv4Addr::from(u32::from_ne_bytes(row[4..8].try_into().unwrap_or([0; 4])));
        let local_port = u16::from_be(u16::from_ne_bytes(row[8..10].try_into().unwrap_or([0; 2])));
        let remote_addr =
            Ipv4Addr::from(u32::from_ne_bytes(row[12..16].try_into().unwrap_or([0; 4])));
        let remote_port =
            u16::from_be(u16::from_ne_bytes(row[16..18].try_into().unwrap_or([0; 2])));
        let pid = u32::from_ne_bytes(row[20..24].try_into().unwrap_or([0; 4]));

        let state = WindowsTcpState::from_mib(state_raw);
        let remote = if state == WindowsTcpState::Listen || remote_addr.is_unspecified() {
            None
        } else {
            Some(SocketAddr::V4(SocketAddrV4::new(remote_addr, remote_port)))
        };

        out.push(WindowsProcessConnection {
            pid,
            protocol: WindowsTransportProtocol::Tcp,
            local_addr: SocketAddr::V4(SocketAddrV4::new(local_addr, local_port)),
            remote_addr: remote,
            state,
        });
    }
    Ok(())
}

#[cfg(windows)]
fn parse_tcp6_table(
    buf: &[u8],
    out: &mut Vec<WindowsProcessConnection>,
) -> Result<(), WindowsApiError> {
    if buf.len() < 4 {
        return Ok(());
    }
    let num_entries = u32::from_ne_bytes(buf[0..4].try_into().unwrap_or([0; 4])) as usize;
    let entries = num_entries.min(MAX_ENTRIES);
    let row_size = 56; // ucLocalAddr(16) + dwLocalScopeId(4) + dwLocalPort(4) + ucRemoteAddr(16) + dwRemoteScopeId(4) + dwRemotePort(4) + dwState(4) + dwOwningPid(4)
    if buf.len() < 4 + entries * row_size {
        return Ok(());
    }
    for i in 0..entries {
        let offset = 4 + i * row_size;
        let row = &buf[offset..offset + row_size];
        let local_addr = Ipv6Addr::from(<[u8; 16]>::try_from(&row[0..16]).unwrap_or([0; 16]));
        let local_port = u16::from_be(u16::from_ne_bytes(row[20..22].try_into().unwrap_or([0; 2])));
        let remote_addr = Ipv6Addr::from(<[u8; 16]>::try_from(&row[24..40]).unwrap_or([0; 16]));
        let remote_port =
            u16::from_be(u16::from_ne_bytes(row[44..46].try_into().unwrap_or([0; 2])));
        let state_raw = u32::from_ne_bytes(row[48..52].try_into().unwrap_or([0; 4]));
        let pid = u32::from_ne_bytes(row[52..56].try_into().unwrap_or([0; 4]));

        let state = WindowsTcpState::from_mib(state_raw);
        let remote = if state == WindowsTcpState::Listen || remote_addr.is_unspecified() {
            None
        } else {
            Some(SocketAddr::V6(SocketAddrV6::new(
                remote_addr,
                remote_port,
                0,
                0,
            )))
        };

        out.push(WindowsProcessConnection {
            pid,
            protocol: WindowsTransportProtocol::Tcp,
            local_addr: SocketAddr::V6(SocketAddrV6::new(local_addr, local_port, 0, 0)),
            remote_addr: remote,
            state,
        });
    }
    Ok(())
}

#[cfg(windows)]
fn parse_udp4_table(
    buf: &[u8],
    out: &mut Vec<WindowsProcessConnection>,
) -> Result<(), WindowsApiError> {
    if buf.len() < 4 {
        return Ok(());
    }
    let num_entries = u32::from_ne_bytes(buf[0..4].try_into().unwrap_or([0; 4])) as usize;
    let entries = num_entries.min(MAX_ENTRIES);
    let row_size = 12; // dwLocalAddr(4) + dwLocalPort(4) + dwOwningPid(4)
    if buf.len() < 4 + entries * row_size {
        return Ok(());
    }
    for i in 0..entries {
        let offset = 4 + i * row_size;
        let row = &buf[offset..offset + row_size];
        let local_addr = Ipv4Addr::from(u32::from_ne_bytes(row[0..4].try_into().unwrap_or([0; 4])));
        let local_port = u16::from_be(u16::from_ne_bytes(row[4..6].try_into().unwrap_or([0; 2])));
        let pid = u32::from_ne_bytes(row[8..12].try_into().unwrap_or([0; 4]));

        out.push(WindowsProcessConnection {
            pid,
            protocol: WindowsTransportProtocol::Udp,
            local_addr: SocketAddr::V4(SocketAddrV4::new(local_addr, local_port)),
            remote_addr: None,
            state: WindowsTcpState::Unknown,
        });
    }
    Ok(())
}

#[cfg(windows)]
fn parse_udp6_table(
    buf: &[u8],
    out: &mut Vec<WindowsProcessConnection>,
) -> Result<(), WindowsApiError> {
    if buf.len() < 4 {
        return Ok(());
    }
    let num_entries = u32::from_ne_bytes(buf[0..4].try_into().unwrap_or([0; 4])) as usize;
    let entries = num_entries.min(MAX_ENTRIES);
    let row_size = 28; // ucLocalAddr(16) + dwLocalScopeId(4) + dwLocalPort(4) + dwOwningPid(4)
    if buf.len() < 4 + entries * row_size {
        return Ok(());
    }
    for i in 0..entries {
        let offset = 4 + i * row_size;
        let row = &buf[offset..offset + row_size];
        let local_addr = Ipv6Addr::from(<[u8; 16]>::try_from(&row[0..16]).unwrap_or([0; 16]));
        let local_port = u16::from_be(u16::from_ne_bytes(row[20..22].try_into().unwrap_or([0; 2])));
        let pid = u32::from_ne_bytes(row[24..28].try_into().unwrap_or([0; 4]));

        out.push(WindowsProcessConnection {
            pid,
            protocol: WindowsTransportProtocol::Udp,
            local_addr: SocketAddr::V6(SocketAddrV6::new(local_addr, local_port, 0, 0)),
            remote_addr: None,
            state: WindowsTcpState::Unknown,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_process_network.rs"]
mod tests;
