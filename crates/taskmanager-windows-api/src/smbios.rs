//! Bounded native raw SMBIOS firmware table reader.

use crate::WindowsApiError;

/// Maximum reasonable size for an SMBIOS table (4 MiB).
const MAX_SMBIOS_BUFFER_BYTES: usize = 4 * 1024 * 1024;

/// Four-character provider code for raw SMBIOS tables ('RSMB': 0x52534D42).
const RSMB_PROVIDER: u32 = u32::from_be_bytes(*b"RSMB");

/// Read the raw SMBIOS firmware table bytes reported by Windows.
///
/// Returns the complete raw SMBIOS structure table byte stream on success.
#[must_use = "inspect the native SMBIOS table query result"]
pub fn raw_smbios_table() -> Result<Vec<u8>, WindowsApiError> {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::{
            FIRMWARE_TABLE_PROVIDER, GetSystemFirmwareTable,
        };

        let provider = FIRMWARE_TABLE_PROVIDER(RSMB_PROVIDER);

        // First query: ask for the required buffer size with `None`.
        let needed_bytes = {
            // SAFETY: Calling `GetSystemFirmwareTable` with `None` as the buffer
            // queries the required byte count without writing memory.
            unsafe { GetSystemFirmwareTable(provider, 0, None) }
        };

        if needed_bytes == 0 {
            return Err(WindowsApiError::QueryFailed);
        }

        let needed_usize = needed_bytes as usize;
        if needed_usize > MAX_SMBIOS_BUFFER_BYTES {
            return Err(WindowsApiError::ResourceLimit);
        }

        let mut buffer = vec![0u8; needed_usize];

        // Second query: fill the allocated buffer.
        let written_bytes = {
            // SAFETY: `buffer` is a valid, contiguous slice of `needed_usize` bytes.
            // The Win32 API copies at most `buffer.len()` bytes synchronously.
            unsafe { GetSystemFirmwareTable(provider, 0, Some(&mut buffer)) }
        };

        if written_bytes == 0 || written_bytes as usize > needed_usize {
            return Err(WindowsApiError::QueryFailed);
        }

        buffer.truncate(written_bytes as usize);
        Ok(buffer)
    }

    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query the processor maximum (turbo) speed in MHz from SMBIOS Type 4
/// (Processor Information) `Max Speed` field. This is the only non-privileged
/// source that reports the boost ceiling on modern hybrid parts where CPUID
/// leaf 0x16 is zero-filled; `Current Speed` is a live value and must never be
/// used as a static base or maximum.
#[must_use = "inspect the SMBIOS processor max speed result"]
pub fn query_smbios_processor_max_mhz() -> Option<u64> {
    let bytes = raw_smbios_table().ok()?;
    if bytes.len() < 8 {
        return None;
    }
    let table_len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let end = (8 + table_len).min(bytes.len());
    let mut offset = 8;

    while offset + 4 <= end {
        let type_id = bytes[offset];
        let length = bytes[offset + 1] as usize;
        if length < 4 || offset + length > end {
            break;
        }

        if type_id == 4 && length >= 0x18 {
            let max_speed = u16::from_le_bytes([bytes[offset + 0x14], bytes[offset + 0x15]]) as u64;
            if max_speed > 0 && max_speed < 10_000 {
                return Some(max_speed);
            }
        }

        if type_id == 127 {
            break;
        }

        offset += length;
        while offset + 1 < end && !(bytes[offset] == 0 && bytes[offset + 1] == 0) {
            offset += 1;
        }
        offset += 2;
    }

    None
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_smbios.rs"]
mod tests;
