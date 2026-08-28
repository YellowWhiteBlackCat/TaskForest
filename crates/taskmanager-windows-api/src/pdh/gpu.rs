//! Windows PDH GPU engine and video-memory queries.

#[cfg(windows)]
use super::counters::{PdhQuery, query_pdh_counter_items};
#[cfg(windows)]
use super::cpu::{
    parse_engine_type_from_instance_name, parse_luid_from_instance_name,
    parse_pid_from_instance_name,
};
#[cfg(windows)]
use super::{
    MAX_PDH_NAME_UTF16, WindowsApiError, WindowsGpuAdapterMemorySample, WindowsGpuEngineDetail,
    WindowsGpuEngineInstanceSample, WindowsGpuEngineSample, WindowsGpuProcessMemorySample,
};

#[cfg(windows)]
pub(super) fn query_gpu_adapter_memory_windows()
-> Result<Vec<WindowsGpuAdapterMemorySample>, WindowsApiError> {
    use std::collections::BTreeMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCollectQueryData,
        PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery::new(query);

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

    let read_counter_map =
        |counter: PDH_HCOUNTER| -> Result<BTreeMap<String, u64>, WindowsApiError> {
            let items_buffer = match query_pdh_counter_items(counter, PDH_FMT_LARGE) {
                Ok(Some(items)) => items,
                Ok(None) => return Ok(BTreeMap::new()),
                Err(error) => return Err(error),
            };
            let items = items_buffer.items();
            let mut map = BTreeMap::new();
            for item in items {
                let Some(name) = items_buffer.decode_name(item, MAX_PDH_NAME_UTF16) else {
                    continue;
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
            Ok(map)
        };

    let dedicated = read_counter_map(dedicated_counter)?;
    let shared = read_counter_map(shared_counter)?;
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
pub(super) fn query_gpu_engine_utilization_windows()
-> Result<Vec<WindowsGpuEngineSample>, WindowsApiError> {
    use std::collections::HashMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCollectQueryData,
        PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery::new(query);

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

    let Some(items_buffer) = query_pdh_counter_items(counter, PDH_FMT_DOUBLE)? else {
        return Ok(Vec::new());
    };
    let items_slice = items_buffer.items();
    let mut luid_engines: HashMap<u64, HashMap<String, f32>> = HashMap::new();

    for item in items_slice {
        let Some(name) = items_buffer.decode_name(item, MAX_PDH_NAME_UTF16) else {
            continue;
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
pub(super) fn query_gpu_engine_instances_windows()
-> Result<Vec<WindowsGpuEngineInstanceSample>, WindowsApiError> {
    use std::collections::BTreeMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCollectQueryData,
        PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery::new(query);

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

    let Some(items_buffer) = query_pdh_counter_items(counter, PDH_FMT_DOUBLE)? else {
        return Ok(Vec::new());
    };
    let items_slice = items_buffer.items();

    // Sibling engine instances of one type (phys_N/eng_M) share the typed
    // row Task Manager shows per process; sum them per (pid, LUID, type).
    let mut rows: BTreeMap<(u32, u64, String), f32> = BTreeMap::new();
    for item in items_slice {
        let Some(name) = items_buffer.decode_name(item, MAX_PDH_NAME_UTF16) else {
            continue;
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
pub(super) fn query_gpu_process_memory_windows()
-> Result<Vec<WindowsGpuProcessMemorySample>, WindowsApiError> {
    use std::collections::BTreeMap;
    use windows::Win32::System::Performance::{
        PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCollectQueryData,
        PdhOpenQueryW,
    };
    use windows::core::w;

    let mut query = PDH_HQUERY::default();
    // SAFETY: PdhOpenQueryW initializes a new PDH query handle.
    let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
    if status != 0 || query.0.is_null() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _query_guard = PdhQuery::new(query);

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
            let Some(items_buffer) = query_pdh_counter_items(counter, PDH_FMT_LARGE)? else {
                return Ok(BTreeMap::new());
            };
            let items = items_buffer.items();
            let mut map: BTreeMap<(u32, u64), u64> = BTreeMap::new();
            for item in items {
                let Some(name) = items_buffer.decode_name(item, MAX_PDH_NAME_UTF16) else {
                    continue;
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
