//! Bounded processor-topology queries for the Windows API boundary.
//!
//! `GetLogicalProcessorInformationEx` returns a variable-length record buffer.
//! The parser below treats every length as untrusted and exposes only the
//! aggregate facts needed by the product: package count and cache capacity.
//! Processor affinity masks and the variable trailing group-mask array never
//! cross this boundary.

use crate::WindowsApiError;

const RELATION_PROCESSOR_CORE: i32 = 0;
const RELATION_CACHE: i32 = 2;
const RELATION_PROCESSOR_PACKAGE: i32 = 3;
const RECORD_HEADER_BYTES: usize = 8;
const CORE_FIXED_BYTES: usize = 32;
const CACHE_FIXED_BYTES: usize = 40;
const MAX_RELATIONSHIP_BYTES: usize = 1024 * 1024;
const MAX_RELATIONSHIP_RECORDS: usize = 4096;

/// Core breakdown facts for hybrid / heterogeneous processors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowsCoreBreakdown {
    pub p_cores: u16,
    pub e_cores: u16,
    pub lp_cores: u16,
    pub smt_cores: u16,
    pub total_physical_cores: u16,
}

/// Logical CPU classification for heterogeneous parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WindowsCpuType {
    Performance,
    Efficient,
    LowPower,
    #[default]
    Unknown,
}

/// Static processor facts returned by the Windows topology relationship API.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowsProcessorTopology {
    /// Number of physical processor packages visible to this process.
    pub socket_count: Option<u16>,
    /// Core breakdown when a hybrid/heterogeneous part is detected.
    pub core_breakdown: Option<WindowsCoreBreakdown>,
    /// Logical core classification for each logical processor (0..N).
    pub cpu_types: Vec<WindowsCpuType>,
    /// Sum of distinct Windows cache relationship records by level, in KiB.
    /// L1 is split by kind (`CacheData`/`CacheUnified` → data slot,
    /// `CacheInstruction` → instruction slot).
    pub l1d_cache_kb: Option<u64>,
    pub l1i_cache_kb: Option<u64>,
    pub l2_cache_kb: Option<u64>,
    pub l3_cache_kb: Option<u64>,
}

/// Query bounded processor-package, core topology, and cache relationship records.
#[must_use = "inspect the processor topology query result"]
pub fn processor_topology() -> Result<WindowsProcessorTopology, WindowsApiError> {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::{
            RelationCache, RelationProcessorCore, RelationProcessorPackage,
        };

        let package_records = query_relationship(RelationProcessorPackage, RECORD_HEADER_BYTES)?;
        let cache_records = query_relationship(RelationCache, RECORD_HEADER_BYTES)?;
        let core_records = query_relationship(RelationProcessorCore, RECORD_HEADER_BYTES)?;
        let package_count = count_records(&package_records, RELATION_PROCESSOR_PACKAGE)?;
        let (core_breakdown, cpu_types, _) =
            parse_core_records(&core_records, RELATION_PROCESSOR_CORE)?;
        let cache = parse_cache_records(&cache_records, RELATION_CACHE)?;
        let socket_count = if package_count == 0 {
            None
        } else {
            Some(u16::try_from(package_count).map_err(|_| WindowsApiError::ResourceLimit)?)
        };

        Ok(WindowsProcessorTopology {
            socket_count,
            core_breakdown,
            cpu_types,
            l1d_cache_kb: cache[0],
            l1i_cache_kb: cache[1],
            l2_cache_kb: cache[2],
            l3_cache_kb: cache[3],
        })
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_relationship(
    relationship: windows::Win32::System::SystemInformation::LOGICAL_PROCESSOR_RELATIONSHIP,
    record_header_size: usize,
) -> Result<Vec<u8>, WindowsApiError> {
    use std::mem::size_of;
    use std::slice;
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };

    if record_header_size < RECORD_HEADER_BYTES {
        return Err(WindowsApiError::QueryFailed);
    }

    let mut required_bytes = 0_u32;
    let _ = {
        // SAFETY: the first call intentionally supplies a null buffer and a
        // valid writable length pointer, as required by the Windows API. The
        // API does not retain either pointer after this synchronous call.
        unsafe { GetLogicalProcessorInformationEx(relationship, None, &mut required_bytes) }
    };
    let required_bytes =
        usize::try_from(required_bytes).map_err(|_| WindowsApiError::QueryFailed)?;
    if !(record_header_size..=MAX_RELATIONSHIP_BYTES).contains(&required_bytes) {
        return Err(WindowsApiError::ResourceLimit);
    }
    let words = required_bytes.div_ceil(size_of::<u64>());
    let mut storage = Vec::<u64>::new();
    storage
        .try_reserve_exact(words)
        .map_err(|_| WindowsApiError::ResourceLimit)?;
    storage.resize(words, 0);
    let storage_bytes = storage
        .len()
        .checked_mul(size_of::<u64>())
        .ok_or(WindowsApiError::ResourceLimit)?;
    let mut returned_bytes =
        u32::try_from(storage_bytes).map_err(|_| WindowsApiError::ResourceLimit)?;
    {
        // SAFETY: `storage` is initialized and u64-aligned, so its pointer is
        // suitable for the generated record type. Its allocation remains live
        // for the synchronous call and is at least the requested byte count.
        let result = unsafe {
            GetLogicalProcessorInformationEx(
                relationship,
                Some(
                    storage
                        .as_mut_ptr()
                        .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
                ),
                &mut returned_bytes,
            )
        };
        if result.is_err() {
            return Err(WindowsApiError::QueryFailed);
        }
    }
    let returned_bytes =
        usize::try_from(returned_bytes).map_err(|_| WindowsApiError::QueryFailed)?;
    if returned_bytes == 0
        || returned_bytes > storage_bytes
        || returned_bytes > MAX_RELATIONSHIP_BYTES
    {
        return Err(WindowsApiError::ResourceLimit);
    }
    // SAFETY: the storage is initialized, aligned, and contains at least the
    // returned byte count. The resulting slice is copied before storage drops.
    let bytes = unsafe { slice::from_raw_parts(storage.as_ptr().cast::<u8>(), returned_bytes) };
    Ok(bytes.to_vec())
}

fn count_records(bytes: &[u8], relationship: i32) -> Result<usize, WindowsApiError> {
    let mut count = 0_usize;
    visit_records(bytes, relationship, |_record| {
        count = count.saturating_add(1);
        Ok(())
    })?;
    Ok(count)
}

fn parse_core_records(
    bytes: &[u8],
    relationship: i32,
) -> Result<(Option<WindowsCoreBreakdown>, Vec<WindowsCpuType>, usize), WindowsApiError> {
    let mut total_physical_cores = 0_u16;
    let mut smt_cores = 0_u16;
    let mut class_0_count = 0_u16;
    let mut class_1_count = 0_u16;
    let mut class_2_count = 0_u16;
    let mut other_class_count = 0_u16;
    let mut logical_types: Vec<WindowsCpuType> = Vec::new();

    visit_records(bytes, relationship, |record| {
        if record.len() < CORE_FIXED_BYTES {
            return Err(WindowsApiError::QueryFailed);
        }
        total_physical_cores = total_physical_cores.saturating_add(1);
        let flags = record[8];
        let efficiency_class = record[9];
        if (flags & 0x01) != 0 {
            smt_cores = smt_cores.saturating_add(1);
        }
        match efficiency_class {
            0 => class_0_count = class_0_count.saturating_add(1),
            1 => class_1_count = class_1_count.saturating_add(1),
            2 => class_2_count = class_2_count.saturating_add(1),
            _ => other_class_count = other_class_count.saturating_add(1),
        }

        if record.len() >= 40 {
            let mask = u64::from_le_bytes([
                record[32], record[33], record[34], record[35], record[36], record[37], record[38],
                record[39],
            ]);
            let cpu_type = match efficiency_class {
                1 => WindowsCpuType::Performance,
                0 => WindowsCpuType::Efficient,
                2 => WindowsCpuType::LowPower,
                _ => WindowsCpuType::Unknown,
            };
            for bit in 0..64 {
                if (mask & (1 << bit)) != 0 {
                    let logical_idx = bit as usize;
                    if logical_idx >= logical_types.len() {
                        logical_types.resize(logical_idx + 1, WindowsCpuType::Unknown);
                    }
                    logical_types[logical_idx] = cpu_type;
                }
            }
        }
        Ok(())
    })?;

    // Check if heterogeneous / hybrid (at least two distinct efficiency classes observed)
    let classes_observed = u8::from(class_0_count > 0)
        + u8::from(class_1_count > 0)
        + u8::from(class_2_count > 0)
        + u8::from(other_class_count > 0);

    let breakdown = if classes_observed > 1 {
        // Hybrid architecture
        let (p_cores, e_cores, lp_cores) = if class_2_count > 0 {
            // 3-tier: Class 1 is P-core, Class 0 is E-core, Class 2 is LP-E core
            (class_1_count, class_0_count, class_2_count)
        } else {
            // 2-tier: Class 1 is P-core, Class 0 is E-core
            (class_1_count, class_0_count, 0)
        };
        Some(WindowsCoreBreakdown {
            p_cores,
            e_cores,
            lp_cores,
            smt_cores,
            total_physical_cores,
        })
    } else {
        None
    };

    let cpu_types = if breakdown.is_some() {
        logical_types
    } else {
        Vec::new()
    };

    Ok((breakdown, cpu_types, total_physical_cores as usize))
}

fn parse_cache_records(
    bytes: &[u8],
    relationship: i32,
) -> Result<[Option<u64>; 4], WindowsApiError> {
    // index 0 = L1 data (incl. unified), 1 = L1 instruction, 2 = L2, 3 = L3.
    let mut totals = [None; 4];
    visit_records(bytes, relationship, |record| {
        if record.len() < CACHE_FIXED_BYTES {
            return Err(WindowsApiError::QueryFailed);
        }
        let level = usize::from(record[8]);
        if !(1..=3).contains(&level) {
            return Ok(());
        }
        let cache_bytes = u64::from(u32::from_le_bytes([
            record[12], record[13], record[14], record[15],
        ]));
        let cache_type = i32::from_le_bytes([record[16], record[17], record[18], record[19]]);
        // CacheUnified=0, CacheInstruction=1, CacheData=2. CacheTrace=3 is
        // an implementation detail, not a capacity row for the CPU page.
        if !(0..=2).contains(&cache_type) {
            return Ok(());
        }
        // L1 splits by kind exactly once per relationship record; a unified
        // L1 reports into the data slot (shown once, never double-counted).
        let slot = match (level, cache_type) {
            (1, 0) | (1, 2) => 0,
            (1, 1) => 1,
            (2, _) => 2,
            (3, _) => 3,
            _ => return Ok(()),
        };
        if cache_bytes == 0 {
            return Ok(());
        }
        let cache_kb = cache_bytes.div_ceil(1024);
        let slot = &mut totals[slot];
        *slot = Some(
            slot.unwrap_or(0_u64)
                .checked_add(cache_kb)
                .ok_or(WindowsApiError::ResourceLimit)?,
        );
        Ok(())
    })?;
    Ok(totals)
}

fn visit_records<F>(bytes: &[u8], relationship: i32, mut visitor: F) -> Result<(), WindowsApiError>
where
    F: FnMut(&[u8]) -> Result<(), WindowsApiError>,
{
    if bytes.is_empty() || bytes.len() > MAX_RELATIONSHIP_BYTES {
        return Err(WindowsApiError::ResourceLimit);
    }
    let mut offset = 0_usize;
    let mut records = 0_usize;
    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        if remaining < RECORD_HEADER_BYTES {
            return Err(WindowsApiError::QueryFailed);
        }
        let record_relationship = i32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        let size = usize::try_from(u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]))
        .map_err(|_| WindowsApiError::QueryFailed)?;
        if !(RECORD_HEADER_BYTES..=remaining).contains(&size) {
            return Err(WindowsApiError::QueryFailed);
        }
        records = records
            .checked_add(1)
            .ok_or(WindowsApiError::ResourceLimit)?;
        if records > MAX_RELATIONSHIP_RECORDS {
            return Err(WindowsApiError::ResourceLimit);
        }
        if record_relationship == relationship {
            visitor(&bytes[offset..offset + size])?;
        }
        offset += size;
    }
    if offset != bytes.len() {
        return Err(WindowsApiError::QueryFailed);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_topology.rs"]
mod tests;
