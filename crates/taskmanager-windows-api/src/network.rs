//! Bounded adapter metadata from the Windows IP Helper table.
//!
//! This boundary deliberately does not call the Native Wi-Fi API. SSID,
//! BSSID, and signal queries are location-protected on current Windows
//! releases, so they must not run as part of a periodic task-manager refresh.

use crate::WindowsApiError;

const MAX_ADAPTERS: usize = 4096;

/// Native classification of a network interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WindowsAdapterType {
    Ethernet,
    WiFi,
    Vpn,
    Virtual,
    Loopback,
    #[default]
    Other,
}

/// Link metadata returned without exposing the native table or interface GUID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsNetworkAdapter {
    pub name: String,
    pub description: String,
    pub adapter_type: WindowsAdapterType,
    pub receive_link_speed_bps: Option<u64>,
    pub transmit_link_speed_bps: Option<u64>,
    pub link_up: Option<bool>,
}

/// Enumerate bounded adapter metadata through `GetIfTable2`.
#[must_use = "inspect the adapter metadata query result"]
pub fn enumerate_network_adapters() -> Result<Vec<WindowsNetworkAdapter>, WindowsApiError> {
    #[cfg(windows)]
    {
        enumerate_network_adapters_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
struct MibTable(*mut windows::Win32::NetworkManagement::IpHelper::MIB_IF_TABLE2);

#[cfg(windows)]
impl Drop for MibTable {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }
        // SAFETY: the pointer was allocated by GetIfTable2 and is released
        // exactly once by this owner; no caller pointer crosses the API.
        unsafe {
            windows::Win32::NetworkManagement::IpHelper::FreeMibTable(self.0.cast());
        }
    }
}

#[cfg(windows)]
fn enumerate_network_adapters_windows() -> Result<Vec<WindowsNetworkAdapter>, WindowsApiError> {
    use std::slice;
    use windows::Win32::NetworkManagement::IpHelper::{GetIfTable2, MIB_IF_TABLE2};

    let mut table = std::ptr::null_mut::<MIB_IF_TABLE2>();
    // SAFETY: `table` is a valid writable out-pointer. Windows allocates
    // the table and its RAII owner below releases it with FreeMibTable.
    let result = unsafe { GetIfTable2(&mut table) };
    if result.is_err() || table.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let table = MibTable(table);
    // SAFETY: GetIfTable2 succeeded and returned a non-null table.
    let count = usize::try_from(unsafe { (*table.0).NumEntries })
        .map_err(|_| WindowsApiError::QueryFailed)?;
    if count > MAX_ADAPTERS {
        return Err(WindowsApiError::ResourceLimit);
    }
    // SAFETY: the Windows table layout contains `NumEntries` contiguous rows
    // after the first inline row; the count is bounded before constructing the
    // slice and the RAII owner keeps the allocation alive for this scope.
    let rows = unsafe { slice::from_raw_parts((*table.0).Table.as_ptr(), count) };
    let mut adapters = Vec::with_capacity(rows.len());
    for row in rows {
        let alias_end = row
            .Alias
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(row.Alias.len());
        if alias_end == 0 {
            return Err(WindowsApiError::InvalidText);
        }
        let name = String::from_utf16(&row.Alias[..alias_end])
            .map_err(|_| WindowsApiError::InvalidText)?;

        let desc_end = row
            .Description
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(row.Description.len());
        let description = String::from_utf16_lossy(&row.Description[..desc_end]);

        // Filter out NDIS filter driver intermediate adapters (e.g. Native WiFi Filter, WFP Filter, QoS Scheduler)
        if name.contains("-WFP ")
            || name.contains("-QoS ")
            || name.contains("-Native WiFi ")
            || name.contains("-0000")
            || name.contains("Filter-")
            || description.contains("-0000")
            || description.contains("Filter-")
            || description.contains("Packet Scheduler")
            || description.contains("LightWeight Filter")
            || description.contains("Extension Filter")
        {
            continue;
        }

        let adapter_type = match row.Type {
            6 => WindowsAdapterType::Ethernet,
            71 => WindowsAdapterType::WiFi,
            24 => WindowsAdapterType::Loopback,
            23 | 131 => WindowsAdapterType::Vpn,
            53 => WindowsAdapterType::Virtual,
            _ => WindowsAdapterType::Other,
        };

        adapters.push(WindowsNetworkAdapter {
            name,
            description,
            adapter_type,
            receive_link_speed_bps: nonzero(row.ReceiveLinkSpeed),
            transmit_link_speed_bps: nonzero(row.TransmitLinkSpeed),
            link_up: link_up_from_status(row.OperStatus.0),
        });
    }
    Ok(adapters)
}

fn nonzero(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

fn link_up_from_status(status: i32) -> Option<bool> {
    match status {
        1 => Some(true),
        2..=7 => Some(false),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_network.rs"]
mod tests;
