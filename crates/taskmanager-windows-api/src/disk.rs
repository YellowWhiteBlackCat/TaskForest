//! Audited disk performance telemetry via Win32 IOCTL_DISK_PERFORMANCE.

use crate::WindowsApiError;

/// Raw performance counters returned by `IOCTL_DISK_PERFORMANCE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WindowsDiskPerformance {
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub read_time_100ns: u64,
    pub write_time_100ns: u64,
    pub idle_time_100ns: u64,
    pub read_count: u32,
    pub write_count: u32,
    pub queue_depth: u32,
    pub query_time_100ns: u64,
}

/// Query real-time disk performance counters for a drive letter (e.g., "C:" or "C:\\").
#[must_use = "inspect disk performance query result"]
pub fn query_disk_performance(drive: &str) -> Result<WindowsDiskPerformance, WindowsApiError> {
    #[cfg(windows)]
    {
        query_disk_performance_windows(drive)
    }
    #[cfg(not(windows))]
    {
        let _ = drive;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_disk_performance_windows(drive: &str) -> Result<WindowsDiskPerformance, WindowsApiError> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    // IOCTL_DISK_PERFORMANCE = CTL_CODE(IOCTL_DISK_BASE, 0x0008, METHOD_BUFFERED, FILE_ANY_ACCESS)
    // IOCTL_DISK_BASE = 0x00000007 => (7 << 16) | (8 << 2) = 0x00070020
    const IOCTL_DISK_PERFORMANCE: u32 = 0x00070020;

    let clean_drive = drive
        .trim()
        .trim_end_matches('\\')
        .trim_end_matches('/')
        .trim_end_matches(':');
    if clean_drive.is_empty() || clean_drive.len() > 32 {
        return Err(WindowsApiError::InvalidInput);
    }
    let device_path = format!("\\\\.\\{clean_drive}:");
    let wide_path: Vec<u16> = std::ffi::OsStr::new(&device_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // Open volume handle without requiring administrator write privileges (0 / query access).
    // SAFETY: `wide_path` is a null-terminated UTF-16 slice.
    let handle = unsafe {
        CreateFileW(
            windows::core::PCWSTR(wide_path.as_ptr()),
            0, // No specific access rights needed for IOCTL_DISK_PERFORMANCE
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };

    let handle = match handle {
        Ok(h) if !h.is_invalid() => h,
        _ => return Err(WindowsApiError::QueryFailed),
    };

    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            // SAFETY: Handle is valid and owned.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let _guard = HandleGuard(handle);

    #[repr(C)]
    #[derive(Default)]
    struct DiskPerformanceRaw {
        bytes_read: i64,
        bytes_written: i64,
        read_time: i64,
        write_time: i64,
        idle_time: i64,
        read_count: u32,
        write_count: u32,
        queue_depth: u32,
        overall_granularity: u32,
        query_time: i64,
        storage_device_number: u32,
        storage_manager_name: [u16; 8],
    }

    let mut raw = DiskPerformanceRaw::default();
    let mut bytes_returned = 0u32;

    // SAFETY: `handle` is valid, `raw` points to memory of size of DiskPerformanceRaw.
    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_PERFORMANCE,
            None,
            0,
            Some(core::ptr::from_mut(&mut raw).cast::<c_void>()),
            size_of::<DiskPerformanceRaw>() as u32,
            Some(&mut bytes_returned),
            None,
        )
    };

    if success.is_err() || bytes_returned != size_of::<DiskPerformanceRaw>() as u32 {
        return Err(WindowsApiError::QueryFailed);
    }

    Ok(WindowsDiskPerformance {
        bytes_read: raw.bytes_read.max(0) as u64,
        bytes_written: raw.bytes_written.max(0) as u64,
        read_time_100ns: raw.read_time.max(0) as u64,
        write_time_100ns: raw.write_time.max(0) as u64,
        idle_time_100ns: raw.idle_time.max(0) as u64,
        read_count: raw.read_count,
        write_count: raw.write_count,
        queue_depth: raw.queue_depth,
        query_time_100ns: raw.query_time.max(0) as u64,
    })
}

/// Interconnect bus type reported by STORAGE_DEVICE_DESCRIPTOR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WindowsDiskBusType {
    Nvme,
    Sata,
    Usb,
    Scsi,
    Sas,
    Mmc,
    Sd,
    Virtual,
    Raid,
    #[default]
    Other,
}

/// Medium type determined by bus type and seek penalty.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WindowsDiskMediaType {
    Ssd,
    Hdd,
    #[default]
    Unknown,
}

/// Device identification and media properties for a drive letter.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WindowsDiskDeviceInfo {
    pub bus_type: WindowsDiskBusType,
    pub media_type: WindowsDiskMediaType,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub product_revision: Option<String>,
    pub serial_number: Option<String>,
    pub is_removable: bool,
}

/// Query hardware bus, media type (SSD/HDD), and model identifiers for a drive letter.
#[must_use = "inspect disk device info query result"]
pub fn query_disk_device_info(drive: &str) -> Result<WindowsDiskDeviceInfo, WindowsApiError> {
    #[cfg(windows)]
    {
        query_disk_device_info_windows(drive)
    }
    #[cfg(not(windows))]
    {
        let _ = drive;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_disk_device_info_windows(drive: &str) -> Result<WindowsDiskDeviceInfo, WindowsApiError> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
        STORAGE_PROPERTY_QUERY, StorageDeviceProperty, StorageDeviceSeekPenaltyProperty,
    };

    let clean_drive = drive
        .trim()
        .trim_end_matches('\\')
        .trim_end_matches('/')
        .trim_end_matches(':');
    if clean_drive.is_empty() || clean_drive.len() > 32 {
        return Err(WindowsApiError::InvalidInput);
    }
    let device_path = format!("\\\\.\\{clean_drive}:");
    let wide_path: Vec<u16> = std::ffi::OsStr::new(&device_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: `wide_path` is a valid null-terminated UTF-16 string.
    let handle = unsafe {
        CreateFileW(
            windows::core::PCWSTR(wide_path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };

    let handle = match handle {
        Ok(h) if !h.is_invalid() => h,
        _ => return Err(WindowsApiError::QueryFailed),
    };

    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            // SAFETY: Handle is valid and owned.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let _guard = HandleGuard(handle);

    let mut query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut out_buffer = [0u8; 1024];
    let mut bytes_returned = 0u32;

    // SAFETY: `handle` is valid, `query` is initialized, `out_buffer` has 1024 bytes.
    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(core::ptr::from_mut(&mut query).cast::<c_void>()),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(out_buffer.as_mut_ptr().cast::<c_void>()),
            out_buffer.len() as u32,
            Some(&mut bytes_returned),
            None,
        )
    };

    let returned_bytes =
        usize::try_from(bytes_returned).map_err(|_| WindowsApiError::ResourceLimit)?;
    if success.is_err() {
        return Err(WindowsApiError::QueryFailed);
    }
    if returned_bytes > out_buffer.len() {
        return Err(WindowsApiError::ResourceLimit);
    }
    if returned_bytes < 32 {
        return Err(WindowsApiError::QueryFailed);
    }

    let is_removable = out_buffer[10] != 0;
    let vendor_id_offset = u32::from_le_bytes(out_buffer[12..16].try_into().unwrap_or_default());
    let product_id_offset = u32::from_le_bytes(out_buffer[16..20].try_into().unwrap_or_default());
    let product_rev_offset = u32::from_le_bytes(out_buffer[20..24].try_into().unwrap_or_default());
    let serial_num_offset = u32::from_le_bytes(out_buffer[24..28].try_into().unwrap_or_default());
    let raw_bus_type = u32::from_le_bytes(out_buffer[28..32].try_into().unwrap_or_default());

    let bus_type = match raw_bus_type {
        17 => WindowsDiskBusType::Nvme,
        11 => WindowsDiskBusType::Sata,
        7 => WindowsDiskBusType::Usb,
        1..=3 => WindowsDiskBusType::Scsi,
        10 => WindowsDiskBusType::Sas,
        12 => WindowsDiskBusType::Sd,
        13 => WindowsDiskBusType::Mmc,
        14..=15 => WindowsDiskBusType::Virtual,
        8 => WindowsDiskBusType::Raid,
        _ => WindowsDiskBusType::Other,
    };

    let extract_str = |offset: u32| -> Option<String> {
        let offset = offset as usize;
        if offset == 0 || offset >= returned_bytes {
            return None;
        }
        let slice = &out_buffer[offset..returned_bytes];
        let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
        let s = std::str::from_utf8(&slice[..end]).ok()?.trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    };

    let vendor_id = extract_str(vendor_id_offset);
    let product_id = extract_str(product_id_offset);
    let product_revision = extract_str(product_rev_offset);
    let serial_number = extract_str(serial_num_offset);

    // Query seek penalty (0 = SSD, 1 = HDD)
    let mut seek_query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut seek_desc = DEVICE_SEEK_PENALTY_DESCRIPTOR::default();
    let mut seek_bytes = 0u32;

    // SAFETY: `handle` is valid, `seek_desc` is a stack struct.
    let seek_ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(core::ptr::from_mut(&mut seek_query).cast::<c_void>()),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(core::ptr::from_mut(&mut seek_desc).cast::<c_void>()),
            size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            Some(&mut seek_bytes),
            None,
        )
    };

    let media_type = if bus_type == WindowsDiskBusType::Nvme {
        WindowsDiskMediaType::Ssd
    } else if seek_ok.is_ok() && seek_bytes as usize == size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>()
    {
        if seek_desc.IncursSeekPenalty {
            WindowsDiskMediaType::Hdd
        } else {
            WindowsDiskMediaType::Ssd
        }
    } else {
        WindowsDiskMediaType::Unknown
    };

    Ok(WindowsDiskDeviceInfo {
        bus_type,
        media_type,
        vendor_id,
        product_id,
        product_revision,
        serial_number,
        is_removable,
    })
}

/// NVMe SMART and health metrics from Win32 IOCTL_STORAGE_QUERY_PROPERTY.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct WindowsDiskSmartInfo {
    pub temperature_c: Option<f32>,
    pub percentage_used: Option<u8>,
    pub critical_warning: u8,
}

/// Query NVMe SMART / health telemetry (temperature, percentage used wear, warning flags).
#[must_use = "inspect disk SMART info query result"]
pub fn query_disk_smart_info(drive: &str) -> Result<WindowsDiskSmartInfo, WindowsApiError> {
    #[cfg(windows)]
    {
        query_disk_smart_info_windows(drive)
    }
    #[cfg(not(windows))]
    {
        let _ = drive;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_disk_smart_info_windows(drive: &str) -> Result<WindowsDiskSmartInfo, WindowsApiError> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::IOCTL_STORAGE_QUERY_PROPERTY;

    const STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY: u32 = 49;
    const STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY: u32 = 50;
    const PROPERTY_STANDARD_QUERY: u32 = 0;
    const PROTOCOL_TYPE_NVME: u32 = 2;
    const NVME_DATA_TYPE_LOG_PAGE: u32 = 2;
    const NVME_LOG_PAGE_HEALTH_INFO: u32 = 0x02;

    #[repr(C)]
    struct StorageProtocolSpecificData {
        protocol_type: u32,
        data_type: u32,
        protocol_data_request_value: u32,
        protocol_data_request_sub_value: u32,
        protocol_data_offset: u32,
        protocol_data_length: u32,
        fixed_protocol_return_data: u32,
        protocol_data_request_sub_value2: u32,
        protocol_data_request_sub_value3: u32,
        protocol_data_request_sub_value4: u32,
    }

    #[repr(C)]
    struct StoragePropertyQueryProtocol {
        property_id: u32,
        query_type: u32,
        protocol_specific: StorageProtocolSpecificData,
    }

    let clean_drive = drive
        .trim()
        .trim_end_matches('\\')
        .trim_end_matches('/')
        .trim_end_matches(':');
    if clean_drive.is_empty() || clean_drive.len() > 32 {
        return Err(WindowsApiError::InvalidInput);
    }
    let device_path = format!("\\\\.\\{clean_drive}:");
    let wide_path: Vec<u16> = std::ffi::OsStr::new(&device_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: `wide_path` is a null-terminated UTF-16 slice.
    let handle = unsafe {
        CreateFileW(
            windows::core::PCWSTR(wide_path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };

    let handle = match handle {
        Ok(h) if !h.is_invalid() => h,
        _ => return Err(WindowsApiError::QueryFailed),
    };

    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            // SAFETY: Handle is valid and owned.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let _guard = HandleGuard(handle);

    const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D1080;
    const IOCTL_STORAGE_PREDICT_FAILURE: u32 = 0x002D1100;

    #[repr(C)]
    struct StorageDeviceNumber {
        device_type: u32,
        device_number: u32,
        partition_number: u32,
    }

    #[repr(C)]
    struct StoragePredictFailure {
        predict_failure: u32,
        vendor_specific: [u8; 512],
    }

    let mut handles_to_try = vec![handle];

    let mut dev_num = StorageDeviceNumber {
        device_type: 0,
        device_number: 0,
        partition_number: 0,
    };
    let mut dev_num_bytes = 0u32;
    // SAFETY: buffers on stack are valid.
    let get_num_ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            None,
            0,
            Some(core::ptr::from_mut(&mut dev_num).cast::<c_void>()),
            size_of::<StorageDeviceNumber>() as u32,
            Some(&mut dev_num_bytes),
            None,
        )
    };

    let mut phys_guard = None;
    if get_num_ok.is_ok() && dev_num_bytes as usize == size_of::<StorageDeviceNumber>() {
        let phys_path = format!("\\\\.\\PhysicalDrive{}", dev_num.device_number);
        let phys_wide: Vec<u16> = std::ffi::OsStr::new(&phys_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `phys_wide` is null-terminated UTF-16.
        let phys_handle = unsafe {
            CreateFileW(
                windows::core::PCWSTR(phys_wide.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        };
        if let Some(h) = phys_handle.ok().filter(|h| !h.is_invalid()) {
            handles_to_try.push(h);
            phys_guard = Some(HandleGuard(h));
        }
    }

    for &h in &handles_to_try {
        // Try device-level first, then adapter-level query
        for prop_id in [
            STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY,
            STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY,
        ] {
            let mut query = StoragePropertyQueryProtocol {
                property_id: prop_id,
                query_type: PROPERTY_STANDARD_QUERY,
                protocol_specific: StorageProtocolSpecificData {
                    protocol_type: PROTOCOL_TYPE_NVME,
                    data_type: NVME_DATA_TYPE_LOG_PAGE,
                    protocol_data_request_value: NVME_LOG_PAGE_HEALTH_INFO,
                    protocol_data_request_sub_value: 0,
                    protocol_data_offset: size_of::<StorageProtocolSpecificData>() as u32,
                    protocol_data_length: 512,
                    fixed_protocol_return_data: 0,
                    protocol_data_request_sub_value2: 0,
                    protocol_data_request_sub_value3: 0,
                    protocol_data_request_sub_value4: 0,
                },
            };

            let mut out_buffer = [0u8; 1024];
            let mut bytes_returned = 0u32;

            // SAFETY: `h` is valid, buffers are properly sized on stack.
            let ok = unsafe {
                DeviceIoControl(
                    h,
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    Some(core::ptr::from_mut(&mut query).cast::<c_void>()),
                    size_of::<StoragePropertyQueryProtocol>() as u32,
                    Some(out_buffer.as_mut_ptr().cast::<c_void>()),
                    out_buffer.len() as u32,
                    Some(&mut bytes_returned),
                    None,
                )
            };

            let returned_bytes = usize::try_from(bytes_returned).unwrap_or(usize::MAX);
            if ok.is_ok() && returned_bytes <= out_buffer.len() && returned_bytes >= 512 {
                let data_offset = if bytes_returned >= 24 {
                    let offset = u32::from_le_bytes([
                        out_buffer[16],
                        out_buffer[17],
                        out_buffer[18],
                        out_buffer[19],
                    ]) as usize;
                    if offset > 0
                        && offset
                            .checked_add(512)
                            .is_some_and(|end| end <= returned_bytes)
                    {
                        offset
                    } else if returned_bytes >= 48 + 512 {
                        48
                    } else {
                        40
                    }
                } else {
                    continue;
                };

                if let Some(data_end) = data_offset.checked_add(512)
                    && data_end <= returned_bytes
                {
                    let log = &out_buffer[data_offset..data_end];
                    let critical_warning = log[0];
                    let kelvin = u16::from_le_bytes([log[1], log[2]]);
                    let temp_c = if (273..=450).contains(&kelvin) {
                        Some((kelvin as f32) - 273.15)
                    } else {
                        None
                    };
                    let percentage_used = Some(log[5]);

                    return Ok(WindowsDiskSmartInfo {
                        temperature_c: temp_c,
                        percentage_used,
                        critical_warning,
                    });
                }
            }
        }

        // Fallback: IOCTL_STORAGE_PREDICT_FAILURE for non-NVMe disks
        let mut predict = StoragePredictFailure {
            predict_failure: 0,
            vendor_specific: [0u8; 512],
        };
        let mut predict_bytes = 0u32;
        // SAFETY: buffer is on stack.
        let predict_ok = unsafe {
            DeviceIoControl(
                h,
                IOCTL_STORAGE_PREDICT_FAILURE,
                None,
                0,
                Some(core::ptr::from_mut(&mut predict).cast::<c_void>()),
                size_of::<StoragePredictFailure>() as u32,
                Some(&mut predict_bytes),
                None,
            )
        };
        if predict_ok.is_ok() && predict_bytes as usize == size_of::<StoragePredictFailure>() {
            return Ok(WindowsDiskSmartInfo {
                temperature_c: None,
                percentage_used: None,
                critical_warning: if predict.predict_failure != 0 { 1 } else { 0 },
            });
        }
    }

    drop(phys_guard);
    Err(WindowsApiError::QueryFailed)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_disk.rs"]
mod tests;
