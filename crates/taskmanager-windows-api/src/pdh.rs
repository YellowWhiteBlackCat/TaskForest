//! Audited native Windows PDH performance counters for GPU engine and CPU frequency.

use crate::WindowsApiError;

/// Breakdown per individual engine type (e.g. 3D, Copy, Video Decode, Compute, Neural).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuEngineDetail {
    pub engine_name: String,
    pub utilization_pct: f32,
}

/// An aggregated GPU utilization sample per adapter LUID.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuEngineSample {
    /// 64-bit adapter LUID (high << 32 | low).
    pub luid: u64,
    /// Total utilization percentage across engines for this adapter (0.0..100.0).
    pub utilization_pct: f32,
    /// Breakdown per engine type.
    pub engines: Vec<WindowsGpuEngineDetail>,
}

/// Dynamic processor frequency readings from PDH.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsCpuFrequencySample {
    /// Aggregate processor frequency across all cores in MHz.
    pub total_frequency_mhz: Option<u64>,
    /// Per-logical-core dynamic frequency in MHz.
    pub per_core_frequency_mhz: Vec<Option<u64>>,
}

/// Per-adapter video-memory usage from `\GPU Adapter Memory(*)` (WDDM 2.0+,
/// the same source Task Manager's GPU memory readouts use).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuAdapterMemorySample {
    /// Adapter instance name reported by PDH (e.g. "Intel(R) Arc(TM) Graphics").
    pub instance_name: String,
    /// 64-bit adapter LUID (high << 32 | low), when the instance name carries one.
    pub luid: Option<u64>,
    /// Current dedicated video-memory usage in bytes, when observed.
    pub dedicated_usage_bytes: Option<u64>,
    /// Current shared system-memory usage in bytes, when observed.
    pub shared_usage_bytes: Option<u64>,
}

/// One per-process GPU engine utilization row from `\GPU Engine(*)`, with the
/// pid, adapter LUID, and engine type parsed out of the PDH instance name and
/// sibling engine instances of the same type summed per (pid, LUID, type).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuEngineInstanceSample {
    pub pid: u32,
    /// 64-bit adapter LUID (high << 32 | low).
    pub luid: u64,
    /// Engine type display label parsed from `engtype_` (e.g. "3D", "Video Decode", "Neural").
    pub engine_type: String,
    pub utilization_pct: f32,
}

/// Per-process dedicated/shared GPU memory from `\GPU Process Memory(*)`
/// (WDDM 2.0+, Task Manager's own per-process GPU memory source), aggregated
/// per (pid, adapter LUID).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsGpuProcessMemorySample {
    pub pid: u32,
    /// 64-bit adapter LUID (high << 32 | low).
    pub luid: u64,
    pub dedicated_bytes: u64,
    pub shared_bytes: u64,
}

/// Query active GPU engine utilization percentages grouped by adapter LUID.
#[must_use = "inspect GPU engine utilization query result"]
pub fn query_gpu_engine_utilization() -> Result<Vec<WindowsGpuEngineSample>, WindowsApiError> {
    #[cfg(windows)]
    {
        query_gpu_engine_utilization_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query per-process GPU engine utilization rows, one aggregated row per
/// (pid, adapter LUID, engine type).
#[must_use = "inspect GPU engine instance query result"]
pub fn query_gpu_engine_instances() -> Result<Vec<WindowsGpuEngineInstanceSample>, WindowsApiError>
{
    #[cfg(windows)]
    {
        query_gpu_engine_instances_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query per-process dedicated/shared GPU memory usage aggregated per
/// (pid, adapter LUID). A host whose WDDM lacks the counter set fails the
/// counter add, which keeps the same typed classification as
/// [`query_gpu_adapter_memory`].
#[must_use = "inspect GPU process memory query result"]
pub fn query_gpu_process_memory() -> Result<Vec<WindowsGpuProcessMemorySample>, WindowsApiError> {
    #[cfg(windows)]
    {
        query_gpu_process_memory_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query current dedicated/shared video-memory usage per GPU adapter via PDH.
#[must_use = "inspect GPU adapter memory query result"]
pub fn query_gpu_adapter_memory() -> Result<Vec<WindowsGpuAdapterMemorySample>, WindowsApiError> {
    #[cfg(windows)]
    {
        query_gpu_adapter_memory_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

/// Query dynamic processor frequencies from Windows performance counters.
#[must_use = "inspect dynamic processor frequency query result"]
pub fn query_cpu_dynamic_frequencies() -> Result<WindowsCpuFrequencySample, WindowsApiError> {
    #[cfg(windows)]
    {
        query_cpu_dynamic_frequencies_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn query_gpu_adapter_memory_windows() -> Result<Vec<WindowsGpuAdapterMemorySample>, WindowsApiError>
{
    use std::collections::BTreeMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY,
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery(query);

    let mut dedicated_counter = PDH_HCOUNTER::default();
    let dedicated_path = w!("\\GPU Adapter Memory(*)\\Dedicated Usage");
    // SAFETY: query is a valid PDH query handle and dedicated_path is a static wide string.
    let status = unsafe { PdhAddEnglishCounterW(query, dedicated_path, 0, &mut dedicated_counter) };
    if status != 0 || dedicated_counter.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }

    let mut shared_counter = PDH_HCOUNTER::default();
    let shared_path = w!("\\GPU Adapter Memory(*)\\Shared Usage");
    // SAFETY: query is a valid PDH query handle and shared_path is a static wide string.
    let status = unsafe { PdhAddEnglishCounterW(query, shared_path, 0, &mut shared_counter) };
    if status != 0 || shared_counter.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }

    // Instantaneous byte counters: one collection is sufficient.
    // SAFETY: query is a valid PDH query handle.
    let status = unsafe { PdhCollectQueryData(query) };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let read_counter_map = |counter: PDH_HCOUNTER| -> BTreeMap<String, u64> {
        let mut buffer_size: u32 = 0;
        let mut item_count: u32 = 0;
        // SAFETY: Passing None/null buffer to query required size.
        let _ = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &mut buffer_size,
                &mut item_count,
                None,
            )
        };
        if buffer_size == 0 || item_count == 0 {
            return BTreeMap::new();
        }
        let mut buffer = vec![0u8; buffer_size as usize];
        let items_ptr = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        // SAFETY: buffer is sized to buffer_size.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &mut buffer_size,
                &mut item_count,
                Some(items_ptr),
            )
        };
        if status != 0 {
            return BTreeMap::new();
        }
        // SAFETY: items_ptr points to initialized array of PDH_FMT_COUNTERVALUE_ITEM_W of length item_count.
        let items = unsafe { std::slice::from_raw_parts(items_ptr, item_count as usize) };
        let mut map = BTreeMap::new();
        for item in items {
            if item.szName.is_null() {
                continue;
            }
            // SAFETY: item.szName is valid non-null pointer within PDH buffer.
            let name = unsafe {
                let mut len = 0;
                while *item.szName.0.add(len) != 0 && len < 512 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(item.szName.0, len);
                String::from_utf16_lossy(slice)
            };
            if name == "_Total" {
                continue;
            }
            // SAFETY: Anonymous union contains largeValue per PDH_FMT_LARGE.
            let value = unsafe { item.FmtValue.Anonymous.largeValue };
            if value >= 0 {
                map.insert(name, value as u64);
            }
        }
        map
    };

    let dedicated = read_counter_map(dedicated_counter);
    let shared = read_counter_map(shared_counter);
    let mut names: std::collections::BTreeSet<&String> =
        dedicated.keys().chain(shared.keys()).collect();
    let mut samples = Vec::with_capacity(names.len());
    while let Some(name) = names.pop_first() {
        samples.push(WindowsGpuAdapterMemorySample {
            instance_name: name.clone(),
            luid: parse_luid_from_instance_name(name),
            dedicated_usage_bytes: dedicated.get(name).copied(),
            shared_usage_bytes: shared.get(name).copied(),
        });
    }
    Ok(samples)
}

#[cfg(windows)]
struct PdhQuery(windows::Win32::System::Performance::PDH_HQUERY);

#[cfg(windows)]
impl Drop for PdhQuery {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: self.0 is a valid PDH query handle returned by PdhOpenQueryW.
            let _ = unsafe { windows::Win32::System::Performance::PdhCloseQuery(self.0) };
        }
    }
}

#[cfg(windows)]
fn query_gpu_engine_utilization_windows() -> Result<Vec<WindowsGpuEngineSample>, WindowsApiError> {
    use std::collections::HashMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery(query);

    let mut counter = PDH_HCOUNTER::default();
    let counter_path = w!("\\GPU Engine(*)\\Utilization Percentage");
    // SAFETY: query is a valid PDH query handle and counter_path is a valid static wide string.
    let status = unsafe { PdhAddEnglishCounterW(query, counter_path, 0, &mut counter) };
    if status != 0 || counter.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }

    // Collect first sample.
    // SAFETY: query is a valid PDH query handle.
    let status = unsafe { PdhCollectQueryData(query) };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    // Small delay to compute delta for rate/percentage counters if needed.
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Collect second sample.
    // SAFETY: query is a valid PDH query handle.
    let status = unsafe { PdhCollectQueryData(query) };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let mut buffer_size: u32 = 0;
    let mut item_count: u32 = 0;
    // First call to determine buffer size.
    // SAFETY: Passing None/null buffer to query required size.
    let _ = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut buffer_size,
            &mut item_count,
            None,
        )
    };

    if buffer_size == 0 || item_count == 0 {
        return Ok(Vec::new());
    }

    let mut buffer: Vec<u8> = vec![0; buffer_size as usize];
    let items_ptr = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();

    // Second call to retrieve the counter items.
    // SAFETY: buffer has length buffer_size and items_ptr is valid writable pointer.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut buffer_size,
            &mut item_count,
            Some(items_ptr),
        )
    };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    // SAFETY: items_ptr points to initialized array of PDH_FMT_COUNTERVALUE_ITEM_W of length item_count.
    let items_slice = unsafe { std::slice::from_raw_parts(items_ptr, item_count as usize) };
    let mut luid_engines: HashMap<u64, HashMap<String, f32>> = HashMap::new();

    for item in items_slice {
        if item.szName.is_null() {
            continue;
        }
        // Decode null-terminated UTF-16 string
        // SAFETY: item.szName is valid non-null pointer within PDH buffer.
        let name = unsafe {
            let mut len = 0;
            while *item.szName.0.add(len) != 0 && len < 512 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(item.szName.0, len);
            String::from_utf16_lossy(slice)
        };

        // Name format: pid_1234_luid_0x00000000_0x0000ABCD_phys_0_eng_0_engtype_3D
        if let Some(luid) = parse_luid_from_instance_name(&name) {
            let eng_type = parse_engine_type_from_instance_name(&name).unwrap_or("3D");
            let eng_map = luid_engines.entry(luid).or_default();
            let eng_entry = eng_map.entry(eng_type.to_string()).or_insert(0.0);
            // SAFETY: Anonymous union contains doubleValue per PDH_FMT_DOUBLE.
            let val = unsafe { item.FmtValue.Anonymous.doubleValue };
            if val.is_finite() && val > 0.0 {
                *eng_entry += val as f32;
            }
        }
    }

    let mut samples = Vec::new();
    for (luid, eng_map) in luid_engines {
        let mut max_util = 0.0f32;
        let mut engines: Vec<WindowsGpuEngineDetail> = eng_map
            .into_iter()
            .map(|(engine_name, u)| {
                let clamped = u.clamp(0.0, 100.0);
                if clamped > max_util {
                    max_util = clamped;
                }
                WindowsGpuEngineDetail {
                    engine_name,
                    utilization_pct: clamped,
                }
            })
            .collect();
        engines.sort_by(|a, b| a.engine_name.cmp(&b.engine_name));
        samples.push(WindowsGpuEngineSample {
            luid,
            utilization_pct: max_util,
            engines,
        });
    }

    Ok(samples)
}

#[cfg(windows)]
const MAX_GPU_ENGINE_INSTANCE_ROWS: usize = 2048;

#[cfg(windows)]
const MAX_GPU_PROCESS_MEMORY_ROWS: usize = 1024;

#[cfg(windows)]
fn query_gpu_engine_instances_windows()
-> Result<Vec<WindowsGpuEngineInstanceSample>, WindowsApiError> {
    use std::collections::BTreeMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery(query);

    let mut counter = PDH_HCOUNTER::default();
    let counter_path = w!("\\GPU Engine(*)\\Utilization Percentage");
    // SAFETY: query is a valid PDH query handle and counter_path is a static wide string.
    let status = unsafe { PdhAddEnglishCounterW(query, counter_path, 0, &mut counter) };
    if status != 0 || counter.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }

    // Percentage counters need two samples to form the timed delta.
    // SAFETY: query is a valid PDH query handle.
    let status = unsafe { PdhCollectQueryData(query) };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    // SAFETY: query is a valid PDH query handle.
    let status = unsafe { PdhCollectQueryData(query) };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let mut buffer_size: u32 = 0;
    let mut item_count: u32 = 0;
    // SAFETY: Passing None/null buffer to query required size.
    let _ = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut buffer_size,
            &mut item_count,
            None,
        )
    };
    if buffer_size == 0 || item_count == 0 {
        return Ok(Vec::new());
    }
    let mut buffer: Vec<u8> = vec![0; buffer_size as usize];
    let items_ptr = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    // SAFETY: buffer has length buffer_size and items_ptr is a valid writable pointer.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut buffer_size,
            &mut item_count,
            Some(items_ptr),
        )
    };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }
    // SAFETY: items_ptr points to an initialized array of PDH_FMT_COUNTERVALUE_ITEM_W of length item_count.
    let items_slice = unsafe { std::slice::from_raw_parts(items_ptr, item_count as usize) };

    // Sibling engine instances of one type (phys_N/eng_M) share the typed
    // row Task Manager shows per process; sum them per (pid, LUID, type).
    let mut rows: BTreeMap<(u32, u64, String), f32> = BTreeMap::new();
    for item in items_slice {
        if item.szName.is_null() {
            continue;
        }
        // SAFETY: item.szName is a valid non-null pointer within the PDH buffer.
        let name = unsafe {
            let mut len = 0;
            while *item.szName.0.add(len) != 0 && len < 512 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(item.szName.0, len);
            String::from_utf16_lossy(slice)
        };
        let (Some(pid), Some(luid)) = (
            parse_pid_from_instance_name(&name),
            parse_luid_from_instance_name(&name),
        ) else {
            continue;
        };
        // SAFETY: Anonymous union contains doubleValue per PDH_FMT_DOUBLE.
        let val = unsafe { item.FmtValue.Anonymous.doubleValue };
        if !val.is_finite() || val <= 0.0 {
            continue;
        }
        let engine_type = parse_engine_type_from_instance_name(&name).unwrap_or("3D");
        let key = (pid, luid, engine_type.to_string());
        if rows.len() >= MAX_GPU_ENGINE_INSTANCE_ROWS && !rows.contains_key(&key) {
            return Err(WindowsApiError::ResourceLimit);
        }
        *rows.entry(key).or_insert(0.0) += val as f32;
    }

    Ok(rows
        .into_iter()
        .map(
            |((pid, luid, engine_type), utilization_pct)| WindowsGpuEngineInstanceSample {
                pid,
                luid,
                engine_type,
                utilization_pct: utilization_pct.clamp(0.0, 100.0),
            },
        )
        .collect())
}

#[cfg(windows)]
fn query_gpu_process_memory_windows() -> Result<Vec<WindowsGpuProcessMemorySample>, WindowsApiError>
{
    use std::collections::BTreeMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY,
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery(query);

    let mut dedicated_counter = PDH_HCOUNTER::default();
    let dedicated_path = w!("\\GPU Process Memory(*)\\Dedicated Usage");
    // SAFETY: query is a valid PDH query handle and dedicated_path is a static wide string.
    let status = unsafe { PdhAddEnglishCounterW(query, dedicated_path, 0, &mut dedicated_counter) };
    if status != 0 || dedicated_counter.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }

    let mut shared_counter = PDH_HCOUNTER::default();
    let shared_path = w!("\\GPU Process Memory(*)\\Shared Usage");
    // SAFETY: query is a valid PDH query handle and shared_path is a static wide string.
    let status = unsafe { PdhAddEnglishCounterW(query, shared_path, 0, &mut shared_counter) };
    if status != 0 || shared_counter.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }

    // Instantaneous byte counters: one collection is sufficient.
    // SAFETY: query is a valid PDH query handle.
    let status = unsafe { PdhCollectQueryData(query) };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let read_counter_map =
        |counter: PDH_HCOUNTER| -> Result<BTreeMap<(u32, u64), u64>, WindowsApiError> {
            let mut buffer_size: u32 = 0;
            let mut item_count: u32 = 0;
            // SAFETY: Passing None/null buffer to query required size.
            let _ = unsafe {
                PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_LARGE,
                    &mut buffer_size,
                    &mut item_count,
                    None,
                )
            };
            if buffer_size == 0 || item_count == 0 {
                return Ok(BTreeMap::new());
            }
            let mut buffer = vec![0u8; buffer_size as usize];
            let items_ptr = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            // SAFETY: buffer is sized to buffer_size.
            let status = unsafe {
                PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_LARGE,
                    &mut buffer_size,
                    &mut item_count,
                    Some(items_ptr),
                )
            };
            if status != 0 {
                return Err(WindowsApiError::QueryFailed);
            }
            // SAFETY: items_ptr points to an initialized array of PDH_FMT_COUNTERVALUE_ITEM_W of length item_count.
            let items = unsafe { std::slice::from_raw_parts(items_ptr, item_count as usize) };
            let mut map: BTreeMap<(u32, u64), u64> = BTreeMap::new();
            for item in items {
                if item.szName.is_null() {
                    continue;
                }
                // SAFETY: item.szName is a valid non-null pointer within the PDH buffer.
                let name = unsafe {
                    let mut len = 0;
                    while *item.szName.0.add(len) != 0 && len < 512 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(item.szName.0, len);
                    String::from_utf16_lossy(slice)
                };
                if name == "_Total" {
                    continue;
                }
                let (Some(pid), Some(luid)) = (
                    parse_pid_from_instance_name(&name),
                    parse_luid_from_instance_name(&name),
                ) else {
                    continue;
                };
                // SAFETY: Anonymous union contains largeValue per PDH_FMT_LARGE.
                let value = unsafe { item.FmtValue.Anonymous.largeValue };
                if value < 0 {
                    continue;
                }
                let key = (pid, luid);
                if map.len() >= MAX_GPU_PROCESS_MEMORY_ROWS && !map.contains_key(&key) {
                    return Err(WindowsApiError::ResourceLimit);
                }
                let bytes = map.entry(key).or_insert(0);
                *bytes = bytes.saturating_add(value as u64);
            }
            Ok(map)
        };

    let dedicated = read_counter_map(dedicated_counter)?;
    let shared = read_counter_map(shared_counter)?;
    let mut rows: BTreeMap<(u32, u64), (u64, u64)> = BTreeMap::new();
    for (key, bytes) in dedicated {
        rows.insert(key, (bytes, 0));
    }
    for (key, bytes) in shared {
        rows.entry(key).or_insert((0, 0)).1 = bytes;
    }

    Ok(rows
        .into_iter()
        .map(
            |((pid, luid), (dedicated_bytes, shared_bytes))| WindowsGpuProcessMemorySample {
                pid,
                luid,
                dedicated_bytes,
                shared_bytes,
            },
        )
        .collect())
}

// Compiled under `cfg(test)` so the pure instance-name parsing rules are
// provable on the Linux CI gate too (same shape as `decode_locale_name`).
#[cfg(any(windows, test))]
fn parse_pid_from_instance_name(name: &str) -> Option<u32> {
    let idx = name.find("pid_")?;
    let rem = &name[idx + 4..];
    rem.split('_').next()?.parse::<u32>().ok()
}

#[cfg(any(windows, test))]
fn parse_engine_type_from_instance_name(name: &str) -> Option<&str> {
    let idx = name.find("engtype_")?;
    let raw = &name[idx + 8..];
    let clean = raw.split('_').next().unwrap_or(raw);
    let mapped = match clean {
        "3D" => "3D",
        "Copy" => "Copy",
        "VideoDecode" => "Video Decode",
        "VideoEncode" => "Video Encode",
        "VideoProcessing" => "Video Processing",
        "Compute" => "Compute",
        "Neural" | "NPU" => "Neural",
        other => other,
    };
    Some(mapped)
}

#[cfg(any(windows, test))]
fn parse_luid_from_instance_name(name: &str) -> Option<u64> {
    // Look for "luid_0x..._0x..."
    let luid_idx = name.find("luid_")?;
    let rem = &name[luid_idx + 5..];
    let mut parts = rem.split('_');
    let high_hex = parts.next()?.strip_prefix("0x")?;
    let low_hex = parts.next()?.strip_prefix("0x")?;
    let high = u32::from_str_radix(high_hex, 16).ok()?;
    let low = u32::from_str_radix(low_hex, 16).ok()?;
    Some(((high as u64) << 32) | (low as u64))
}

#[cfg(windows)]
fn parse_processor_core_index(name: &str) -> Option<Option<usize>> {
    if name == "_Total" || name.ends_with(",_Total") {
        return Some(None);
    }
    let parts: Vec<&str> = name.split(',').collect();
    if parts.len() == 2 {
        let core_str = parts[1].trim();
        if core_str == "_Total" {
            Some(None)
        } else {
            core_str.parse::<usize>().ok().map(Some)
        }
    } else if parts.len() == 1 {
        parts[0].trim().parse::<usize>().ok().map(Some)
    } else {
        None
    }
}

#[cfg(windows)]
fn query_cpu_dynamic_frequencies_windows() -> Result<WindowsCpuFrequencySample, WindowsApiError> {
    use std::collections::BTreeMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery(query);

    let mut freq_counter = PDH_HCOUNTER::default();
    let freq_path = w!("\\Processor Information(*)\\Processor Frequency");
    // SAFETY: query is valid and freq_path is static wide string.
    let _ = unsafe { PdhAddEnglishCounterW(query, freq_path, 0, &mut freq_counter) };

    let mut perf_pct_counter = PDH_HCOUNTER::default();
    let perf_pct_path = w!("\\Processor Information(*)\\% Processor Performance");
    // SAFETY: query is valid and perf_pct_path is static wide string.
    let _ = unsafe { PdhAddEnglishCounterW(query, perf_pct_path, 0, &mut perf_pct_counter) };

    // Initial collection
    // SAFETY: query is valid.
    let status = unsafe { PdhCollectQueryData(query) };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    // Delay for rate/percentage calculation between ticks
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Second collection
    // SAFETY: query is valid.
    let status = unsafe { PdhCollectQueryData(query) };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let read_counter_map = |counter: PDH_HCOUNTER| -> (Option<f64>, BTreeMap<usize, f64>) {
        if counter.0.is_null() {
            return (None, BTreeMap::new());
        }
        let mut buffer_size: u32 = 0;
        let mut item_count: u32 = 0;
        // SAFETY: query required buffer size.
        let _ = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                None,
            )
        };
        if buffer_size == 0 || item_count == 0 {
            return (None, BTreeMap::new());
        }
        let mut buffer = vec![0u8; buffer_size as usize];
        let items_ptr = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        // SAFETY: buffer is sized to buffer_size.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                Some(items_ptr),
            )
        };
        if status != 0 {
            return (None, BTreeMap::new());
        }
        // SAFETY: items_ptr points to initialized array of PDH_FMT_COUNTERVALUE_ITEM_W of length item_count.
        let items = unsafe { std::slice::from_raw_parts(items_ptr, item_count as usize) };
        let mut total_val = None;
        let mut core_map = BTreeMap::new();
        for item in items {
            if item.szName.is_null() {
                continue;
            }
            // SAFETY: item.szName is valid non-null pointer within PDH buffer.
            let name = unsafe {
                let mut len = 0;
                while *item.szName.0.add(len) != 0 && len < 256 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(item.szName.0, len);
                String::from_utf16_lossy(slice)
            };
            // SAFETY: Anonymous union contains doubleValue per PDH_FMT_DOUBLE.
            let val = unsafe { item.FmtValue.Anonymous.doubleValue };
            if !val.is_finite() {
                continue;
            }
            match parse_processor_core_index(&name) {
                Some(None) => {
                    if total_val.is_none() || name == "_Total" {
                        total_val = Some(val);
                    }
                }
                Some(Some(idx)) => {
                    core_map.insert(idx, val);
                }
                None => {}
            }
        }
        (total_val, core_map)
    };

    let (total_freq, freq_map) = read_counter_map(freq_counter);
    let (total_perf, perf_map) = read_counter_map(perf_pct_counter);

    // Per-core base from PROCESSOR_POWER_INFORMATION.MaxMhz (per-core-type
    // base: P 1900 / E 1500 / LP-E 1500). This is the multiplier Task Manager
    // uses; the `Processor Frequency` counter is already a live value and must
    // not be multiplied again.
    let base_by_core: BTreeMap<usize, u64> = crate::query_processor_power_information()
        .ok()
        .into_iter()
        .flatten()
        .filter(|info| info.max_mhz > 0)
        .map(|info| (info.core_number as usize, info.max_mhz as u64))
        .collect();
    let default_base_mhz = base_by_core.values().copied().max();

    // Calculate per-core dynamic MHz (ordered by numeric core index 0, 1, 2...)
    let mut per_core_frequency_mhz = Vec::new();
    let max_core_opt = perf_map
        .keys()
        .max()
        .or_else(|| freq_map.keys().max())
        .or_else(|| base_by_core.keys().max());
    if let Some(&max_core_idx) = max_core_opt {
        for idx in 0..=max_core_idx {
            let base = base_by_core.get(&idx).copied();
            let perf = perf_map.get(&idx).copied();
            let current_freq = freq_map.get(&idx).copied();
            per_core_frequency_mhz.push(per_core_current_mhz(base, perf, current_freq));
        }
    }

    // Calculate total dynamic MHz: prefer arithmetic mean of actual core frequencies
    let total_frequency_mhz = if !per_core_frequency_mhz.is_empty() {
        let valid_cores: Vec<u64> = per_core_frequency_mhz.iter().filter_map(|&f| f).collect();
        if !valid_cores.is_empty() {
            Some(valid_cores.iter().sum::<u64>() / valid_cores.len() as u64)
        } else {
            total_frequency_from_counters(default_base_mhz, total_perf, total_freq)
        }
    } else {
        total_frequency_from_counters(default_base_mhz, total_perf, total_freq)
    };

    Ok(WindowsCpuFrequencySample {
        total_frequency_mhz,
        per_core_frequency_mhz,
    })
}

/// Task Manager's current-frequency algorithm: per-core base ×
/// `% Processor Performance`. Falls back to the `Processor Frequency` counter
/// (already a live MHz value) when the performance ratio is unavailable.
fn per_core_current_mhz(
    base: Option<u64>,
    perf_pct: Option<f64>,
    current_freq_mhz: Option<f64>,
) -> Option<u64> {
    if let (Some(base), Some(perf)) = (base, perf_pct)
        && base > 0
        && perf > 0.0
    {
        return Some((base as f64 * perf / 100.0).round() as u64);
    }
    current_freq_mhz
        .filter(|f| f.is_finite() && *f > 0.0)
        .map(|f| f.round() as u64)
}

/// Aggregate fallback: base × total `% Processor Performance`, otherwise the
/// total `Processor Frequency` counter, otherwise honest absence.
fn total_frequency_from_counters(
    default_base_mhz: Option<u64>,
    total_perf: Option<f64>,
    total_freq: Option<f64>,
) -> Option<u64> {
    if let (Some(base), Some(perf)) = (default_base_mhz, total_perf)
        && base > 0
        && perf > 0.0
    {
        return Some((base as f64 * perf / 100.0).round() as u64);
    }
    total_freq
        .filter(|f| f.is_finite() && *f > 0.0)
        .map(|f| f.round() as u64)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_pdh.rs"]
mod tests;
