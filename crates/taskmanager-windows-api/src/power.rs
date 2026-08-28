//! Audited power scheme queries for the Windows API boundary.
//!
//! Exposes only typed strings for the active power scheme name (e.g. "Balanced",
//! "High Performance", "Power Saver"). Memory allocated by Win32 power APIs is
//! immediately freed via LocalFree before returning.

use crate::WindowsApiError;

const MAX_SCHEME_NAME_BYTES: usize = 1024;

/// Query the friendly display name of the currently active Windows power scheme.
#[must_use = "inspect the active power scheme query result"]
pub fn active_power_scheme_name() -> Result<String, WindowsApiError> {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        use windows::Win32::Foundation::{HLOCAL, LocalFree, WIN32_ERROR};
        use windows::Win32::System::Power::{PowerGetActiveScheme, PowerReadFriendlyName};

        let mut p_guid: *mut windows::core::GUID = std::ptr::null_mut();
        // SAFETY: PowerGetActiveScheme allocates a GUID using LocalAlloc and sets p_guid.
        let status = unsafe { PowerGetActiveScheme(None, &mut p_guid) };
        if status != WIN32_ERROR(0) || p_guid.is_null() {
            return Err(WindowsApiError::QueryFailed);
        }

        struct GuidGuard(*mut windows::core::GUID);
        impl Drop for GuidGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    // SAFETY: The GUID was allocated by PowerGetActiveScheme via LocalAlloc.
                    let _ = unsafe { LocalFree(Some(HLOCAL(self.0.cast::<c_void>()))) };
                }
            }
        }
        let _guard = GuidGuard(p_guid);

        let mut buffer_size: u32 = 0;
        // First call with null buffer to query required size in bytes.
        // SAFETY: p_guid is valid and guarded.
        let status = unsafe {
            PowerReadFriendlyName(None, Some(p_guid), None, None, None, &mut buffer_size)
        };
        if status != WIN32_ERROR(0) || buffer_size == 0 {
            return Err(WindowsApiError::QueryFailed);
        }
        let size_usize =
            usize::try_from(buffer_size).map_err(|_| WindowsApiError::ResourceLimit)?;
        if size_usize == 0 || size_usize > MAX_SCHEME_NAME_BYTES || !size_usize.is_multiple_of(2) {
            return Err(WindowsApiError::ResourceLimit);
        }

        let mut buffer = vec![0_u8; size_usize];
        // Second call to retrieve the UTF-16 string.
        // SAFETY: buffer has length size_usize and p_guid is valid.
        let status = unsafe {
            PowerReadFriendlyName(
                None,
                Some(p_guid),
                None,
                None,
                Some(buffer.as_mut_ptr()),
                &mut buffer_size,
            )
        };
        if status != WIN32_ERROR(0) {
            return Err(WindowsApiError::QueryFailed);
        }
        let returned_bytes =
            usize::try_from(buffer_size).map_err(|_| WindowsApiError::ResourceLimit)?;
        if returned_bytes == 0 || returned_bytes > buffer.len() || !returned_bytes.is_multiple_of(2)
        {
            return Err(WindowsApiError::QueryFailed);
        }

        // Decode UTF-16 from bytes
        let u16_len = returned_bytes / 2;
        let mut u16_vec = Vec::with_capacity(u16_len);
        for chunk in buffer[..returned_bytes].as_chunks::<2>().0 {
            u16_vec.push(u16::from_le_bytes(*chunk));
        }
        // Strip trailing null if present
        while let Some(&0) = u16_vec.last() {
            u16_vec.pop();
        }
        String::from_utf16(&u16_vec).map_err(|_| WindowsApiError::InvalidText)
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Map the effective power overlay GUID to its stable slider-mode label.
///
/// The four documented performance-power-slider values come from
/// "Customize the Windows performance power slider" (learn.microsoft.com):
/// Better Battery `961cc777-2547-4f9d-8174-7d86181b8a7a`, Better Performance
/// `3af9b8d9-7c97-431d-ad78-34a8bfea439f`, Best Performance
/// `ded574b5-45a0-4f42-8737-46345c09c238`. `PowerGetEffectiveOverlayScheme`
/// reports the Balanced personality GUID `381b4222-f694-41f0-9685-ff5bb260df2e`
/// and the all-zero "no overlay" GUID for the default slider mode, both of
/// which are the Better Performance position. Any other GUID is `None` — an
/// unknown overlay is typed absence, never a guess.
#[must_use = "an unmapped overlay GUID is typed absence"]
pub const fn power_overlay_label(
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
) -> Option<&'static str> {
    match (data1, data2, data3, data4) {
        (0x961c_c777, 0x2547, 0x4f9d, [0x81, 0x74, 0x7d, 0x86, 0x18, 0x1b, 0x8a, 0x7a]) => {
            Some("better battery")
        }
        (0x3af9_b8d9, 0x7c97, 0x431d, [0xad, 0x78, 0x34, 0xa8, 0xbf, 0xea, 0x43, 0x9f])
        | (0x381b_4222, 0xf694, 0x41f0, [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e])
        | (0, 0, 0, [0, 0, 0, 0, 0, 0, 0, 0]) => Some("better performance"),
        (0xded5_74b5, 0x45a0, 0x4f42, [0x87, 0x37, 0x46, 0x34, 0x5c, 0x09, 0xc2, 0x38]) => {
            Some("best performance")
        }
        _ => None,
    }
}

/// Query the effective power overlay (Windows performance power slider mode)
/// via `PowerGetEffectiveOverlayScheme` (powrprof, Windows 10 1709+). Returns
/// the stable label from [`power_overlay_label`]; `Ok(None)` means the query
/// succeeded but the returned GUID is not one of the documented slider
/// overlays, which must surface as typed absence rather than a guess.
#[must_use = "inspect the effective power overlay query result"]
pub fn effective_power_overlay_name() -> Result<Option<String>, WindowsApiError> {
    #[cfg(windows)]
    {
        // `windows` 0.62 does not wrap this documented powrprof entry point,
        // so the audited boundary declares it directly via the same link
        // macro the crate itself uses.
        windows::core::link! {
            "powrprof.dll" "system" fn PowerGetEffectiveOverlayScheme(
                effectiveschemeguid: *mut windows::core::GUID,
            ) -> windows::Win32::Foundation::WIN32_ERROR
        }
        let mut overlay = windows::core::GUID::zeroed();
        let status = {
            // SAFETY: `overlay` is a writable, caller-owned GUID; the
            // function writes only this fixed-size value and allocates
            // nothing, so there is nothing to free afterwards.
            unsafe { PowerGetEffectiveOverlayScheme(&mut overlay) }
        };
        use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, WIN32_ERROR};
        if status == ERROR_ACCESS_DENIED {
            return Err(WindowsApiError::PermissionDenied);
        }
        if status != WIN32_ERROR(0) {
            return Err(WindowsApiError::QueryFailed);
        }
        Ok(
            power_overlay_label(overlay.data1, overlay.data2, overlay.data3, overlay.data4)
                .map(str::to_string),
        )
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Typed per-core power and frequency information from `CallNtPowerInformation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WindowsProcessorPowerInfo {
    pub core_number: u32,
    pub max_mhz: u32,
    pub current_mhz: u32,
    pub mhz_limit: u32,
    pub max_idle_state: u32,
    pub current_idle_state: u32,
}

/// Query per-core dynamic frequencies and idle states via `CallNtPowerInformation`.
#[must_use = "inspect processor power information query result"]
pub fn query_processor_power_information() -> Result<Vec<WindowsProcessorPowerInfo>, WindowsApiError>
{
    #[cfg(windows)]
    {
        query_processor_power_information_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_processor_power_information_windows()
-> Result<Vec<WindowsProcessorPowerInfo>, WindowsApiError> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::NTSTATUS;
    use windows::Win32::System::Power::{
        CallNtPowerInformation, PROCESSOR_POWER_INFORMATION, ProcessorInformation,
    };

    let core_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(64)
        .min(512);
    let mut raw_records = vec![PROCESSOR_POWER_INFORMATION::default(); core_count];
    let buffer_bytes = u32::try_from(raw_records.len() * size_of::<PROCESSOR_POWER_INFORMATION>())
        .map_err(|_| WindowsApiError::ResourceLimit)?;

    // SAFETY: `raw_records` is a valid heap buffer of length `buffer_bytes`.
    let status = unsafe {
        CallNtPowerInformation(
            ProcessorInformation,
            None,
            0,
            Some(raw_records.as_mut_ptr().cast::<c_void>()),
            buffer_bytes,
        )
    };

    if status != NTSTATUS(0) {
        return Err(WindowsApiError::QueryFailed);
    }

    let infos = raw_records
        .into_iter()
        .map(|r| WindowsProcessorPowerInfo {
            core_number: r.Number,
            max_mhz: r.MaxMhz,
            current_mhz: r.CurrentMhz,
            mhz_limit: r.MhzLimit,
            max_idle_state: r.MaxIdleState,
            current_idle_state: r.CurrentIdleState,
        })
        .collect();

    Ok(infos)
}

/// Detailed AC line and battery power status from GetSystemPowerStatus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WindowsSystemPowerStatus {
    pub ac_online: Option<bool>,
    pub is_charging: bool,
    pub battery_life_percent: Option<u8>,
    pub battery_saver_active: bool,
    pub battery_life_time_seconds: Option<u32>,
    pub battery_full_life_time_seconds: Option<u32>,
    pub has_battery: bool,
}

/// Query system power and battery status via GetSystemPowerStatus.
#[must_use = "inspect the system power status query result"]
pub fn query_system_power_status() -> Result<WindowsSystemPowerStatus, WindowsApiError> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

        let mut status = SYSTEM_POWER_STATUS::default();
        // SAFETY: `status` is a valid stack-allocated structure.
        let ok = unsafe { GetSystemPowerStatus(&mut status) };
        if ok.is_err() {
            return Err(WindowsApiError::QueryFailed);
        }

        let ac_online = match status.ACLineStatus {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };

        let is_charging = (status.BatteryFlag & 8) != 0;
        let has_battery = (status.BatteryFlag & 128) == 0 && status.BatteryFlag != 255;

        let battery_life_percent = if status.BatteryLifePercent <= 100 {
            Some(status.BatteryLifePercent)
        } else {
            None
        };

        let battery_saver_active = status.SystemStatusFlag == 1;

        let battery_life_time_seconds = if status.BatteryLifeTime != u32::MAX {
            Some(status.BatteryLifeTime)
        } else {
            None
        };

        let battery_full_life_time_seconds = if status.BatteryFullLifeTime != u32::MAX {
            Some(status.BatteryFullLifeTime)
        } else {
            None
        };

        Ok(WindowsSystemPowerStatus {
            ac_online,
            is_charging,
            battery_life_percent,
            battery_saver_active,
            battery_life_time_seconds,
            battery_full_life_time_seconds,
            has_battery,
        })
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_power.rs"]
mod tests;
