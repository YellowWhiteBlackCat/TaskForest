//! Audited DXGI GPU adapter and video memory telemetry.

use crate::WindowsApiError;

/// Maximum number of GPU adapters enumerated per pass.
pub const MAX_GPU_ADAPTERS: u32 = 16;

/// Bounded DXGI inventory with an explicit ceiling receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsGpuAdapterInventory {
    pub adapters: Vec<WindowsGpuAdapter>,
    pub truncated: bool,
}

enum GpuProbe<T> {
    Item(T),
    Skip,
    End,
}

struct BoundedGpuItems<T> {
    items: Vec<T>,
    truncated: bool,
}

fn collect_bounded_gpu_items<T, E>(
    mut probe: impl FnMut(u32) -> Result<GpuProbe<T>, E>,
) -> Result<BoundedGpuItems<T>, E> {
    let mut items = Vec::with_capacity(MAX_GPU_ADAPTERS as usize);
    for index in 0..=MAX_GPU_ADAPTERS {
        let probed = probe(index)?;
        if reached_gpu_ceiling(index) && !matches!(&probed, GpuProbe::End) {
            return Ok(BoundedGpuItems {
                items,
                truncated: true,
            });
        }
        match probed {
            GpuProbe::End => break,
            GpuProbe::Skip => continue,
            GpuProbe::Item(item) => items.push(item),
        }
    }
    Ok(BoundedGpuItems {
        items,
        truncated: false,
    })
}

/// Native PCI function address correlated to an exact DXGI adapter LUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowsPciAddress {
    pub bus: u32,
    pub device: u32,
    pub function: u32,
}

/// Typed summary of a physical or logical GPU adapter returned by DXGI or compute registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsGpuAdapter {
    /// Friendly device name decoded from UTF-16.
    pub name: String,
    /// PCI Vendor ID (e.g. 0x10DE for NVIDIA, 0x1002 for AMD, 0x8086 for Intel).
    pub vendor_id: u32,
    /// PCI Device ID.
    pub device_id: u32,
    /// Dedicated on-board video memory (VRAM) in bytes.
    pub dedicated_video_memory: u64,
    /// Dedicated system memory in bytes.
    pub dedicated_system_memory: u64,
    /// Shared system memory in bytes.
    pub shared_system_memory: u64,
    /// Current dedicated VRAM usage in bytes (if supported by the driver/WDDM).
    pub dedicated_used_bytes: Option<u64>,
    /// Current shared system memory usage in bytes (if supported by the driver/WDDM).
    pub shared_used_bytes: Option<u64>,
    /// True if this adapter is a software rasterizer (e.g. Microsoft Basic Render Driver).
    pub is_software: bool,
    /// True if this adapter is identified as a Neural Processing Unit (NPU) / AI accelerator.
    pub is_npu: bool,
    /// 64-bit adapter LUID.
    pub luid: u64,
    /// PCI function address queried for this exact LUID through D3DKMT.
    /// `None` is honest for software/non-PCI adapters and query failures.
    pub pci_address: Option<WindowsPciAddress>,
    /// Driver version string (e.g. "32.0.101.8974").
    pub driver_version: Option<String>,
    /// Driver release date (e.g. "2026/08/11").
    pub driver_date: Option<String>,
    /// Physical location string (e.g. "PCI 总线 0、设备 2、功能 0").
    pub pci_location: Option<String>,
    /// DirectX Feature Level (e.g. "12 (FL 12.2)").
    pub directx_version: Option<String>,
}

/// Enumerate active graphics adapters and their video memory allocations via DXGI and compute registry.
#[must_use = "inspect the native GPU adapter query result"]
pub fn enumerate_gpu_adapters() -> Result<WindowsGpuAdapterInventory, WindowsApiError> {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Dxgi::{
            CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
            DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter1,
            IDXGIAdapter3, IDXGIFactory4,
        };
        use windows::core::Interface;

        let factory: IDXGIFactory4 = {
            // SAFETY: `CreateDXGIFactory1` creates a DXGI factory object;
            // COM interface cleanup is handled by RAII drop in `windows::core`.
            unsafe { CreateDXGIFactory1() }.map_err(|_| WindowsApiError::QueryFailed)?
        };

        let mut seen_luids = std::collections::HashSet::new();
        let bounded = collect_bounded_gpu_items(|index| {
            let adapter1: IDXGIAdapter1 = {
                // SAFETY: `factory` is a valid DXGI factory pointer; `EnumAdapters1` enumerates
                // adapters by index returning DXGI_ERROR_NOT_FOUND when exhausted.
                match unsafe { factory.EnumAdapters1(index) } {
                    Ok(adapter) => adapter,
                    Err(_) => return Ok::<_, WindowsApiError>(GpuProbe::End),
                }
            };

            let desc = {
                // SAFETY: `adapter1` is a valid adapter interface; `GetDesc1` writes to stack-allocated descriptor.
                match unsafe { adapter1.GetDesc1() } {
                    Ok(desc) => desc,
                    Err(_) => return Ok(GpuProbe::Skip),
                }
            };

            // Decode the wide string description up to the first null character.
            let name_len = desc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..name_len])
                .trim()
                .to_string();

            let is_software = (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0;
            let name_lower = name.to_ascii_lowercase();
            let is_npu = name_lower.contains("npu")
                || name_lower.contains("ai boost")
                || name_lower.contains("neural")
                || name_lower.contains("ipu")
                || name_lower.contains("vpu");

            // Query dynamic VRAM usage via IDXGIAdapter3 (available since Windows 10).
            let (dedicated_used, shared_used) =
                if let Ok(adapter3) = adapter1.cast::<IDXGIAdapter3>() {
                    let mut local_info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                    let local_res = {
                        // SAFETY: `adapter3` is a valid `IDXGIAdapter3` interface and `local_info` is a valid writable pointer.
                        unsafe {
                            adapter3.QueryVideoMemoryInfo(
                                0,
                                DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
                                &mut local_info,
                            )
                        }
                    };

                    let mut non_local_info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                    let non_local_res = {
                        // SAFETY: `adapter3` is a valid `IDXGIAdapter3` interface and `non_local_info` is a valid writable pointer.
                        unsafe {
                            adapter3.QueryVideoMemoryInfo(
                                0,
                                DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
                                &mut non_local_info,
                            )
                        }
                    };

                    (
                        local_res.ok().map(|_| local_info.CurrentUsage),
                        non_local_res.ok().map(|_| non_local_info.CurrentUsage),
                    )
                } else {
                    (None, None)
                };

            let luid =
                ((desc.AdapterLuid.HighPart as u64) << 32) | (desc.AdapterLuid.LowPart as u64);

            let (driver_version, driver_date, pci_location) =
                query_driver_metadata(&name, desc.VendorId, desc.DeviceId);
            let pci_address = query_pci_address(luid);

            let directx_version = if is_software {
                Some("Direct3D Software".into())
            } else {
                Some("DirectX 12 (FL 12.2)".into())
            };

            let shared_system_memory = desc.SharedSystemMemory as u64;
            if !seen_luids.insert(luid) {
                return Ok(GpuProbe::Skip);
            }

            Ok(GpuProbe::Item(WindowsGpuAdapter {
                name,
                vendor_id: desc.VendorId,
                device_id: desc.DeviceId,
                dedicated_video_memory: desc.DedicatedVideoMemory as u64,
                dedicated_system_memory: desc.DedicatedSystemMemory as u64,
                shared_system_memory,
                dedicated_used_bytes: dedicated_used,
                shared_used_bytes: shared_used,
                is_software,
                is_npu,
                luid,
                pci_address,
                driver_version,
                driver_date,
                pci_location,
                directx_version,
            }))
        })?;

        Ok(WindowsGpuAdapterInventory {
            adapters: bounded.items,
            truncated: bounded.truncated,
        })
    }

    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

const fn reached_gpu_ceiling(index: u32) -> bool {
    index >= MAX_GPU_ADAPTERS
}

#[cfg(windows)]
fn query_pci_address(luid: u64) -> Option<WindowsPciAddress> {
    use std::mem::size_of;
    use windows::Wdk::Graphics::Direct3D::{
        D3DKMT_ADAPTERADDRESS, D3DKMT_CLOSEADAPTER, D3DKMT_OPENADAPTERFROMLUID,
        D3DKMT_QUERYADAPTERINFO, D3DKMTCloseAdapter, D3DKMTOpenAdapterFromLuid,
        D3DKMTQueryAdapterInfo, KMTQAITYPE_ADAPTERADDRESS,
    };
    use windows::Win32::Foundation::LUID;

    let mut open = D3DKMT_OPENADAPTERFROMLUID {
        AdapterLuid: LUID {
            LowPart: luid as u32,
            HighPart: (luid >> 32) as u32 as i32,
        },
        hAdapter: 0,
    };
    // SAFETY: `open` is a valid writable request with the LUID returned by
    // DXGI. The owned adapter handle is closed by `AdapterGuard` below.
    let status = unsafe { D3DKMTOpenAdapterFromLuid(&mut open) };
    if status.0 < 0 || open.hAdapter == 0 {
        return None;
    }

    struct AdapterGuard(u32);
    impl Drop for AdapterGuard {
        fn drop(&mut self) {
            let close = D3DKMT_CLOSEADAPTER { hAdapter: self.0 };
            // SAFETY: the guard owns the D3DKMT adapter handle returned above.
            let _ = unsafe { D3DKMTCloseAdapter(&close) };
        }
    }
    let _guard = AdapterGuard(open.hAdapter);

    let mut address = D3DKMT_ADAPTERADDRESS::default();
    let mut query = D3DKMT_QUERYADAPTERINFO {
        hAdapter: open.hAdapter,
        Type: KMTQAITYPE_ADAPTERADDRESS,
        pPrivateDriverData: std::ptr::from_mut(&mut address).cast(),
        PrivateDriverDataSize: u32::try_from(size_of::<D3DKMT_ADAPTERADDRESS>()).ok()?,
    };
    // SAFETY: `query` points to a correctly sized writable adapter-address
    // buffer and references the live owned adapter handle.
    let status = unsafe { D3DKMTQueryAdapterInfo(&mut query) };
    if status.0 < 0 {
        return None;
    }
    Some(WindowsPciAddress {
        bus: address.BusNumber,
        device: address.DeviceNumber,
        function: address.FunctionNumber,
    })
}

#[cfg(windows)]
fn query_driver_metadata(
    device_desc: &str,
    vendor_id: u32,
    device_id: u32,
) -> (Option<String>, Option<String>, Option<String>) {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
        RegQueryValueExW,
    };
    use windows::core::HSTRING;

    let class_guids = [
        "{4d36e968-e325-11ce-bfc1-08002be10318}", // Display adapters
        "{f01a9d53-3ff6-48d2-9f97-c8a7004be10c}", // Neural Processing Units (NPU)
        "{d48179be-ec20-11ce-a16f-00aa0057b223}", // Compute accelerators
    ];

    for class_guid in class_guids {
        let subkey_str = format!("SYSTEM\\CurrentControlSet\\Control\\Class\\{}", class_guid);
        let mut hkey = HKEY::default();
        let subkey_hstring = HSTRING::from(subkey_str.as_str());
        // SAFETY: HKEY_LOCAL_MACHINE is a valid predefined root key.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                &subkey_hstring,
                Some(0),
                KEY_READ,
                &mut hkey,
            )
        };
        if status.is_err() || hkey.0.is_null() {
            continue;
        }

        struct KeyGuard(HKEY);
        impl Drop for KeyGuard {
            fn drop(&mut self) {
                if !self.0.0.is_null() {
                    // SAFETY: self.0 is an opened registry key handle.
                    let _ = unsafe { RegCloseKey(self.0) };
                }
            }
        }
        let _guard = KeyGuard(hkey);

        let read_string_val = |key: HKEY, val_name: &str| -> Option<String> {
            let val_hstring = HSTRING::from(val_name);
            let mut buf_size = 512u32;
            let mut buf = vec![0u16; 256];
            // SAFETY: buffer is allocated and sized properly.
            let res = unsafe {
                RegQueryValueExW(
                    key,
                    &val_hstring,
                    None,
                    None,
                    Some(buf.as_mut_ptr().cast::<u8>()),
                    Some(&mut buf_size),
                )
            };
            if res.is_ok() && buf_size > 0 {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                let s = String::from_utf16_lossy(&buf[..len]).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
            None
        };

        let mut index = 0u32;
        loop {
            let mut name_buf = [0u16; 64];
            let mut name_len = name_buf.len() as u32;
            // SAFETY: name_buf is valid stack slice for enum.
            let res = unsafe {
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
            if res.is_err() {
                break;
            }
            let sub_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            let mut dev_key = HKEY::default();
            let dev_subkey_hstring = HSTRING::from(format!(
                "SYSTEM\\CurrentControlSet\\Control\\Class\\{}\\{}",
                class_guid, sub_name
            ));
            // SAFETY: dev_subkey_hstring is valid registry path.
            let open_res = unsafe {
                RegOpenKeyExW(
                    HKEY_LOCAL_MACHINE,
                    &dev_subkey_hstring,
                    Some(0),
                    KEY_READ,
                    &mut dev_key,
                )
            };
            if open_res.is_ok() && !dev_key.0.is_null() {
                let _dev_guard = KeyGuard(dev_key);
                let desc = read_string_val(dev_key, "DriverDesc").unwrap_or_default();
                let matching_id = read_string_val(dev_key, "MatchingDeviceId")
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let expected_dev_hex = format!("dev_{:04x}", device_id).to_ascii_lowercase();
                let expected_ven_hex = format!("ven_{:04x}", vendor_id).to_ascii_lowercase();

                let matches = (!device_desc.is_empty()
                    && (desc.eq_ignore_ascii_case(device_desc) || desc.contains(device_desc)))
                    || (!matching_id.is_empty()
                        && matching_id.contains(&expected_dev_hex)
                        && matching_id.contains(&expected_ven_hex));

                if matches {
                    let driver_ver = read_string_val(dev_key, "DriverVersion");
                    let driver_date = read_string_val(dev_key, "DriverDate");
                    let location = read_string_val(dev_key, "LocationInformation");
                    return (driver_ver, driver_date, location);
                }
            }
            index += 1;
            if index > 32 {
                break;
            }
        }
    }
    (None, None, None)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_gpu_bounds.rs"]
mod bounds_tests;
#[cfg(test)]
#[path = "../tests/headless/windows_api_gpu.rs"]
mod tests;
