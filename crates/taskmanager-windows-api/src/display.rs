//! Static monitor inventory: EnumDisplayDevices + cached registry EDID.
//!
//! `EnumDisplayDevicesW` maps each active display adapter output to its
//! monitor instance; the monitor's cached EDID block lives at
//! `HKLM\SYSTEM\CurrentControlSet\Enum\<monitor instance>\Device
//! Parameters\EDID` as `REG_BINARY` and is readable by a standard user.
//! The registry value is the OS-cached EDID (an `EDID_OVERRIDE` value
//! shadows it, remote/virtual displays may lack it) — an absent block is a
//! typed `None`, never a fabricated one. EDID *parsing* is pure code and
//! stays adapter-side; this boundary only returns the raw bytes.
//!
//! Current mode/refresh/HDR state is runtime compositor state
//! (`DisplayRuntimeInfo`), not this static inventory.

use crate::WindowsApiError;

/// Maximum number of display adapters enumerated per pass.
const MAX_DISPLAY_ADAPTERS: u32 = 16;
/// Maximum number of monitors enumerated per adapter.
const MAX_MONITORS_PER_ADAPTER: u32 = 8;
/// One base block plus at most 32 extension blocks; anything larger is not a
/// plausible cached EDID value.
const MAX_EDID_BYTES: usize = 128 * 33;

/// One monitor attached to a display output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsMonitorDescriptor {
    /// GDI device name of the owning output (e.g. `\\.\DISPLAY1`).
    pub device_name: String,
    /// Monitor device instance path (the registry Enum key tail).
    pub monitor_instance: Option<String>,
    /// Whether the output reports an attached desktop.
    pub is_active: bool,
    /// Raw cached EDID bytes when present and plausibly sized.
    pub edid: Option<Vec<u8>>,
}

/// Enumerate monitors across all display outputs.
#[cfg(windows)]
pub fn enumerate_display_monitors() -> Result<Vec<WindowsMonitorDescriptor>, WindowsApiError> {
    use windows::Win32::Graphics::Gdi::{
        DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICEW, EnumDisplayDevicesW,
    };
    use windows::core::PCWSTR;

    let struct_bytes = u32::try_from(std::mem::size_of::<DISPLAY_DEVICEW>())
        .map_err(|_| WindowsApiError::ResourceLimit)?;
    let mut monitors: Vec<WindowsMonitorDescriptor> = Vec::new();

    for adapter_index in 0..=MAX_DISPLAY_ADAPTERS {
        let mut adapter = DISPLAY_DEVICEW {
            cb: struct_bytes,
            ..DISPLAY_DEVICEW::default()
        };
        // SAFETY: `adapter` is a valid writable `DISPLAYDEVICEW` whose `cb`
        // matches its allocated size; a null device name enumerates display
        // adapters; flags 0 keeps the legacy GDI names; no pointer is
        // retained past this synchronous call.
        // SAFETY: `adapter` is initialized to the documented struct size and
        // remains live for this synchronous enumeration call.
        let present = {
            // SAFETY: `adapter` remains live and writable for this
            // synchronous enumeration call.
            unsafe { EnumDisplayDevicesW(PCWSTR::null(), adapter_index, &mut adapter, 0) }.as_bool()
        };
        if !present {
            // FALSE ends the adapter enumeration; zero adapters is an honest
            // empty inventory, not a failure.
            break;
        }
        if adapter_index == MAX_DISPLAY_ADAPTERS {
            return Err(WindowsApiError::ResourceLimit);
        }
        let device_name = wide_field_string(&adapter.DeviceName);
        if device_name.is_empty() {
            continue;
        }
        // Keep a private NUL-terminated copy so the monitor pass has a stable
        // pointer independent of the reused struct.
        let mut adapter_name_utf16 = Vec::with_capacity(adapter.DeviceName.len() + 1);
        adapter_name_utf16.extend_from_slice(&adapter.DeviceName);
        adapter_name_utf16.push(0);

        for monitor_index in 0..=MAX_MONITORS_PER_ADAPTER {
            let mut monitor = DISPLAY_DEVICEW {
                cb: struct_bytes,
                ..DISPLAY_DEVICEW::default()
            };
            // SAFETY: `monitor` is a valid writable `DISPLAYDEVICEW` sized by
            // `cb`; `adapter_name_utf16` is NUL-terminated and alive for this
            // synchronous call; flags 0 keeps the legacy names.
            // SAFETY: `monitor` is initialized to the documented struct size;
            // the adapter-name buffer remains live for this synchronous call.
            let present = unsafe {
                EnumDisplayDevicesW(
                    PCWSTR(adapter_name_utf16.as_ptr()),
                    monitor_index,
                    &mut monitor,
                    0,
                )
            }
            .as_bool();
            if !present {
                break;
            }
            if monitor_index == MAX_MONITORS_PER_ADAPTER {
                return Err(WindowsApiError::ResourceLimit);
            }
            let is_active = (monitor.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP).0 != 0;
            let instance = wide_field_string(&monitor.DeviceID);
            let edid = if is_active && !instance.is_empty() {
                read_cached_edid(&instance)
            } else {
                None
            };
            monitors.push(WindowsMonitorDescriptor {
                device_name: device_name.clone(),
                monitor_instance: (!instance.is_empty()).then_some(instance),
                is_active,
                edid,
            });
        }
    }
    Ok(monitors)
}

/// Non-Windows hosts keep the lane dormant with the typed fallback.
#[cfg(not(windows))]
pub fn enumerate_display_monitors() -> Result<Vec<WindowsMonitorDescriptor>, WindowsApiError> {
    Err(WindowsApiError::Unsupported)
}

/// Decode a fixed-width UTF-16 API field up to its terminating NUL.
#[cfg(windows)]
fn wide_field_string(field: &[u16]) -> String {
    let end = field
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(field.len());
    String::from_utf16_lossy(&field[..end]).trim().to_owned()
}

/// Read the OS-cached `REG_BINARY` EDID value for one monitor instance. Any
/// absent, mistyped, or implausibly sized value is a typed `None`; the value
/// is never resized, padded, or synthesized to look like an EDID block.
#[cfg(windows)]
fn read_cached_edid(monitor_instance: &str) -> Option<Vec<u8>> {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_BINARY, REG_VALUE_TYPE, RegCloseKey, RegOpenKeyExW,
        RegQueryValueExW,
    };
    use windows::core::HSTRING;

    struct KeyGuard(HKEY);
    impl Drop for KeyGuard {
        fn drop(&mut self) {
            if !self.0.0.is_null() {
                // SAFETY: the guard owns a registry key handle opened above.
                let _ = unsafe { RegCloseKey(self.0) };
            }
        }
    }

    let subkey = HSTRING::from(format!(
        "SYSTEM\\CurrentControlSet\\Enum\\{monitor_instance}\\Device Parameters"
    ));
    let mut key = HKEY::default();
    // SAFETY: `HKEY_LOCAL_MACHINE` is a valid predefined root; `subkey` is a
    // well-formed NUL-terminated path derived from the enumeration API; the
    // opened handle is owned by `KeyGuard` and closed exactly once.
    let opened = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, &subkey, Some(0), KEY_READ, &mut key) };
    if opened.is_err() || key.0.is_null() {
        return None;
    }
    let key = KeyGuard(key);

    let value_name = HSTRING::from("EDID");
    let mut value_type = REG_VALUE_TYPE(0);
    let mut byte_size = 0_u32;
    // SAFETY: a null data buffer with a valid size pointer requests only the
    // value's type and size; no data is written.
    let probed = unsafe {
        RegQueryValueExW(
            key.0,
            &value_name,
            None,
            Some(&mut value_type),
            None,
            Some(&mut byte_size),
        )
    };
    if probed.is_err() || value_type != REG_BINARY {
        return None;
    }
    let length = usize::try_from(byte_size).ok()?;
    if !(128..=MAX_EDID_BYTES).contains(&length) {
        return None;
    }
    let mut bytes = vec![0_u8; length];
    // SAFETY: `bytes` is `length` bytes long, matching the probed size; the
    // key handle is still owned by `key`.
    let read = unsafe {
        RegQueryValueExW(
            key.0,
            &value_name,
            None,
            None,
            Some(bytes.as_mut_ptr()),
            Some(&mut byte_size),
        )
    };
    if read.is_err() || byte_size as usize != bytes.len() {
        return None;
    }
    Some(bytes)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_display.rs"]
mod tests;
