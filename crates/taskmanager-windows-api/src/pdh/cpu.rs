//! Windows PDH processor-frequency query and instance-name parsing.

#[cfg(windows)]
use super::counters::{PdhQuery, query_pdh_counter_items};
#[cfg(windows)]
use super::{MAX_PDH_NAME_UTF16, MAX_PROCESSOR_CORES, WindowsApiError, WindowsCpuFrequencySample};

// Compiled under `cfg(test)` so the pure instance-name parsing rules are
// provable on the Linux CI gate too (same shape as `decode_locale_name`).
#[cfg(any(windows, test))]
pub(super) fn parse_pid_from_instance_name(name: &str) -> Option<u32> {
    let idx = name.find("pid_")?;
    let rem = &name[idx + 4..];
    rem.split('_').next()?.parse::<u32>().ok()
}

#[cfg(any(windows, test))]
pub(super) fn parse_engine_type_from_instance_name(name: &str) -> Option<&str> {
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
pub(super) fn parse_luid_from_instance_name(name: &str) -> Option<u64> {
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
            core_str
                .parse::<usize>()
                .ok()
                .filter(|index| *index < MAX_PROCESSOR_CORES)
                .map(Some)
        }
    } else if parts.len() == 1 {
        parts[0]
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|index| *index < MAX_PROCESSOR_CORES)
            .map(Some)
    } else {
        None
    }
}

#[cfg(windows)]
pub(super) fn query_cpu_dynamic_frequencies_windows()
-> Result<WindowsCpuFrequencySample, WindowsApiError> {
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

    let read_counter_map =
        |counter: PDH_HCOUNTER| -> Result<(Option<f64>, BTreeMap<usize, f64>), WindowsApiError> {
            if counter.0.is_null() {
                return Ok((None, BTreeMap::new()));
            }
            let items_buffer = match query_pdh_counter_items(counter, PDH_FMT_DOUBLE) {
                Ok(Some(items)) => items,
                Ok(None) => return Ok((None, BTreeMap::new())),
                Err(error) => return Err(error),
            };
            let items = items_buffer.items();
            let mut total_val = None;
            let mut core_map = BTreeMap::new();
            for item in items {
                let Some(name) = items_buffer.decode_name(item.szName, MAX_PDH_NAME_UTF16) else {
                    continue;
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
            Ok((total_val, core_map))
        };

    let (total_freq, freq_map) = read_counter_map(freq_counter)?;
    let (total_perf, perf_map) = read_counter_map(perf_pct_counter)?;

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
pub(super) fn per_core_current_mhz(
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
pub(super) fn total_frequency_from_counters(
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
