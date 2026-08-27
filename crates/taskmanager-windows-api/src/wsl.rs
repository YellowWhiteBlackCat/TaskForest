//! WSL2 and Linux container environment inventory from the safe registry API.

use crate::WindowsApiError;

/// A registered WSL / LXSS distribution on the Windows host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsWslDistro {
    pub name: String,
    pub version: u32,
    pub base_path: Option<String>,
    pub is_default: bool,
}

/// Enumerate registered WSL distributions from HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss.
#[must_use = "inspect WSL distribution enumeration result"]
pub fn query_wsl_distributions() -> Result<Vec<WindowsWslDistro>, WindowsApiError> {
    #[cfg(windows)]
    {
        query_wsl_distributions_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_wsl_distributions_windows() -> Result<Vec<WindowsWslDistro>, WindowsApiError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, REG_SZ, RegCloseKey, RegEnumKeyExW,
        RegOpenKeyExW, RegQueryValueExW,
    };
    use windows::core::PCWSTR;

    struct KeyGuard(HKEY);
    impl Drop for KeyGuard {
        fn drop(&mut self) {
            // SAFETY: Handle is owned and valid.
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let subkey_wide = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Lxss");
    let mut hkey = HKEY::default();
    // SAFETY: standard RegOpenKeyExW call.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey_wide.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        )
    };
    if status.is_err() || hkey.is_invalid() {
        return Ok(Vec::new());
    }
    let _guard = KeyGuard(hkey);

    let get_string_val = |key: HKEY, val_name: &str| -> Option<String> {
        let name_wide = to_wide(val_name);
        let mut data_type = REG_SZ;
        let mut data_bytes = [0u8; 512];
        let mut data_len = data_bytes.len() as u32;
        // SAFETY: buffer and key are valid.
        let status = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name_wide.as_ptr()),
                None,
                Some(&mut data_type),
                Some(data_bytes.as_mut_ptr()),
                Some(&mut data_len),
            )
        };
        if status.is_ok() && data_len >= 2 {
            // SAFETY: data_bytes is valid stack buffer and data_len bytes are initialized.
            let u16_slice = unsafe {
                core::slice::from_raw_parts(
                    data_bytes.as_ptr().cast::<u16>(),
                    (data_len as usize) / 2,
                )
            };
            let end = u16_slice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(u16_slice.len());
            String::from_utf16(&u16_slice[..end]).ok()
        } else {
            None
        }
    };

    let get_u32_val = |key: HKEY, val_name: &str| -> Option<u32> {
        let name_wide = to_wide(val_name);
        let mut data_type = REG_DWORD;
        let mut val = 0u32;
        let mut data_len = size_of::<u32>() as u32;
        // SAFETY: buffer and key are valid.
        let status = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name_wide.as_ptr()),
                None,
                Some(&mut data_type),
                Some(core::ptr::from_mut(&mut val).cast::<u8>()),
                Some(&mut data_len),
            )
        };
        if status.is_ok() { Some(val) } else { None }
    };

    let default_guid = get_string_val(hkey, "DefaultDistribution");

    let mut distros = Vec::new();
    let mut index = 0u32;
    loop {
        let mut name_buf = [0u16; 256];
        let mut name_len = name_buf.len() as u32;
        // SAFETY: name_buf is valid.
        let enum_res = unsafe {
            RegEnumKeyExW(
                hkey,
                index,
                Some(windows::core::PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if enum_res.is_err() {
            break;
        }
        index += 1;

        let guid_str = match String::from_utf16(&name_buf[..name_len as usize]) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let mut distro_hkey = HKEY::default();
        let guid_wide = to_wide(&guid_str);
        // SAFETY: guid_wide is null-terminated.
        let open_res = unsafe {
            RegOpenKeyExW(
                hkey,
                PCWSTR(guid_wide.as_ptr()),
                None,
                KEY_READ,
                &mut distro_hkey,
            )
        };
        if open_res.is_ok() && !distro_hkey.is_invalid() {
            let _distro_guard = KeyGuard(distro_hkey);
            if let Some(name) =
                get_string_val(distro_hkey, "DistributionName").filter(|n| !n.is_empty())
            {
                let version = get_u32_val(distro_hkey, "Version").unwrap_or(2);
                let base_path = get_string_val(distro_hkey, "BasePath");
                let is_default = default_guid.as_deref() == Some(&guid_str);
                distros.push(WindowsWslDistro {
                    name,
                    version,
                    base_path,
                    is_default,
                });
            }
        }
    }

    Ok(distros)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_wsl.rs"]
mod tests;
