//! Shared presentation: command/page help metadata, monochrome icon glyphs,
//! and the single-source byte/duration formatters every frontend renders
//! (ADR-020 single-source rule; the GPUI, TUI, iced, and Bevy frontends all call
//! these — never a per-frontend copy).

use std::collections::BTreeSet;

use taskforest_core::ServiceId;
use taskforest_core::core::metrics::{GpuMetrics, SmbiosMemorySnapshot};
use taskforest_core::core::process::{PriorityTier, ProcessBatchAction};
use taskforest_core::core::services::{
    ServiceItem, detect_ordering_cycles, detect_requirement_cycles,
};
use taskforest_core::core::time::LocalTimeRulesObservation;
use taskforest_core::core::units::{QuantityFamily, UnitPreferences};
pub mod about;
mod constants;
mod cpu;
pub mod diagnostics;
pub mod first_run;
pub mod gpu_chart_metric;
pub mod gpu_engine_rows;
pub mod health_review;
mod help;
pub mod history_replay;
mod network;
pub mod privilege_center;
mod process;
mod service_exit;
pub mod smart;
pub mod smart_tool;
mod storage;
pub mod storage_health;
pub mod system_information;
pub mod system_timeline;
mod telemetry;
pub mod trend;

pub use constants::MISSING_VALUE;
pub use cpu::*;
pub use help::*;
pub use network::*;
pub use process::*;
pub use storage::*;
use taskforest_core::core::metrics::ResourcePressure;
use taskmanager_application::i18n;
use taskmanager_application::process_details_vm::format_local_timestamp_seconds;
pub use telemetry::*;

/// Stable service IDs participating in an observed ordering or strict
/// requirement cycle. The graph algorithms remain owned by core; this shell
/// helper only folds the inventory into the set every renderer can consume.
#[must_use]
pub fn service_cycle_members<'a>(
    services: impl IntoIterator<Item = &'a ServiceItem>,
) -> BTreeSet<ServiceId> {
    let services = services.into_iter().collect::<Vec<_>>();
    let mut members = BTreeSet::new();
    for cycle in detect_ordering_cycles(
        services
            .iter()
            .map(|service| (&service.id, service.relations())),
    )
    .into_iter()
    .chain(detect_requirement_cycles(
        services
            .iter()
            .map(|service| (&service.id, service.relations())),
        true,
    )) {
        members.extend(cycle.path);
    }
    members
}

/// Owned form of [`MISSING_VALUE`] for `String` readouts.
#[must_use]
pub fn bytes(value: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = value as f64;
    if value >= GIB {
        format!("{:.1} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{value:.0} B")
    }
}

/// Format an optional resource value without collapsing an unavailable
/// observation into the numeric zero. This is shared by the TUI and Iced
/// Apps projections for typed PSS and swap columns.
#[must_use]
pub fn optional_bytes(value: Option<u64>) -> String {
    value.map_or_else(missing_value, bytes)
}

/// Format a duration using the active locale's shared hour/minute template.
/// Numeric precision and zero-padding are owned by application i18n.
#[must_use]
pub fn duration(seconds: u64) -> String {
    i18n::format_duration(seconds)
}

/// Format an optional count (threads / fds) without collapsing an unavailable
/// observation into a believable zero: `None` renders an honest dash.
#[must_use]
pub fn optional_count(value: Option<u32>) -> String {
    value.map_or_else(missing_value, |count| count.to_string())
}

/// Format a ResourcePressure (some/full) into a human-readable display string,
/// e.g. "some 0.5% · full 0.1%" or "some 0.0%".
#[must_use]
pub fn format_resource_pressure(pressure: Option<&ResourcePressure>) -> String {
    let Some(pressure) = pressure else {
        return missing_value();
    };
    let some = format!("{:.1}%", pressure.some.avg10);
    if let Some(full) = &pressure.full {
        format!("some {some} · full {:.1}%", full.avg10)
    } else {
        format!("some {some}")
    }
}

/// Format an optional second-duration (CPU time) through the shared
/// [`duration`] helper, with `None` rendering an honest dash.
#[must_use]
pub fn optional_duration(value: Option<u64>) -> String {
    value.map_or_else(missing_value, duration)
}

/// Format a process CPU time as the whole-second cell the terminal and Bevy
/// tables render: `12s`, so a measured zero stays a number instead of
/// collapsing into the missing-value dash. This is the SINGLE whole-seconds
/// spelling (ADR-020); the width-bounded mixed-unit ladder lives in
/// [`cpu_time_compact`]. (`{:.1}` on the `u64` source the frontends held was
/// an integer minimum-digit spec, not a decimal place — the visible text was
/// always a whole second count, and stays exactly that.)
#[must_use]
pub fn cpu_time_seconds(seconds: u64) -> String {
    format!("{seconds}s")
}

/// [`cpu_time_seconds`] with `None` rendering the honest dash.
#[must_use]
pub fn optional_cpu_time_seconds(value: Option<u64>) -> String {
    value.map_or_else(missing_value, cpu_time_seconds)
}

/// Format a process CPU time as the width-bounded ladder the desktop process
/// table renders: `{d}d {h}h` past a day, `{h}h {m}m` past an hour, `{m}m {s}s`
/// past a minute, `{s}s` otherwise. Segments are never zero-padded (the
/// ladder grows left instead), and a process that has accumulated nothing yet
/// renders the shared dash rather than a believable `0s`. This is the SINGLE
/// compact-ladder spelling (ADR-020).
#[must_use]
pub fn cpu_time_compact(seconds: u64) -> String {
    if seconds == 0 {
        return missing_value();
    }
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if days > 0 {
        // Drop the minutes segment once a process is at least a day old: the
        // CpuTime column is width-bounded, and "{days}d {hours}h" stays well
        // inside it even for multi-year accumulations.
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// [`cpu_time_compact`] with `None` rendering the honest dash.
#[must_use]
pub fn optional_cpu_time_compact(value: Option<u64>) -> String {
    value.map_or_else(missing_value, cpu_time_compact)
}

/// Format an optional niceness value, signing a positive priority (`+10`) so it
/// is visually distinct from a measured zero; `None` renders an honest dash.
#[must_use]
pub fn optional_nice(value: Option<i32>) -> String {
    value.map_or_else(missing_value, |nice| {
        if nice > 0 {
            format!("+{nice}")
        } else {
            nice.to_string()
        }
    })
}

/// Locale label for a scheduling-priority tier — the SINGLE fold (§8.1 同一律)
/// shared by every frontend's menus, toasts, and confirmations. The tier
/// word, not a raw Unix nice number, is the honest cross-platform phrasing
/// (each platform adapter maps the tier to its native primitive).
#[must_use]
pub fn priority_tier_label(tier: PriorityTier) -> &'static str {
    i18n::t(tier.i18n_key())
}

/// Localized label for one neutral process-batch action. This is the single
/// action→display fold used by confirmations, menus, and completion feedback
/// in every frontend. Platform words such as a raw nice value or signal name
/// never enter this projection.
#[must_use]
pub fn process_batch_action_label(action: ProcessBatchAction) -> String {
    match action {
        ProcessBatchAction::End => i18n::t("proc.end_task").to_owned(),
        ProcessBatchAction::EndProcessTree => i18n::t("proc.end_process_tree").to_owned(),
        ProcessBatchAction::Kill => i18n::t("proc.kill").to_owned(),
        ProcessBatchAction::Suspend => i18n::t("proc.suspend").to_owned(),
        ProcessBatchAction::Resume => i18n::t("proc.resume").to_owned(),
        ProcessBatchAction::SetPriority(tier) => format!(
            "{} ({})",
            i18n::t("proc.priority"),
            priority_tier_label(tier)
        ),
        ProcessBatchAction::SetEfficiencyMode(enable) => {
            if enable {
                format!(
                    "{} ({})",
                    i18n::t("proc.efficiency_mode"),
                    i18n::t("common.on")
                )
            } else {
                format!(
                    "{} ({})",
                    i18n::t("proc.efficiency_mode"),
                    i18n::t("common.off")
                )
            }
        }
    }
}

/// Format an injected Unix process start instant as the user's local `HH:MM`.
/// Missing rules, a missing/zero epoch, and out-of-range values are all honest
/// dashes; UTC is used only when the injected rule set itself is fixed UTC.
#[must_use]
pub fn start_clock_local(epoch_secs: Option<u64>, rules: &LocalTimeRulesObservation) -> String {
    let Some(epoch_secs) = epoch_secs.filter(|seconds| *seconds != 0) else {
        return missing_value();
    };
    let Some(epoch_secs) = i64::try_from(epoch_secs).ok() else {
        return missing_value();
    };
    rules
        .date_time_at(epoch_secs)
        .map_or_else(missing_value, |local| {
            format!("{:02}:{:02}", local.hour(), local.minute())
        })
}

/// Format injected epoch milliseconds as a local civil timestamp. A local-
/// time capability failure stays a dash instead of relabeling UTC as local.
#[must_use]
pub fn local_timestamp(epoch_millis: u64, rules: &LocalTimeRulesObservation) -> String {
    let epoch_seconds = epoch_millis / 1_000;
    format_local_timestamp_seconds(epoch_seconds, rules).unwrap_or_else(missing_value)
}

/// Format a temperature in °C the way every badge/graph readout does
/// (whole degrees). This is the SINGLE °C spelling (ADR-020) — frontends
/// must not re-format the unit.
#[must_use]
pub fn temperature_c(value: f32) -> String {
    format!("{value:.0} °C")
}

/// Renderer-neutral GPU identity for titles and device rails.
///
/// A resolved product name is the most specific identity, while `brand` is a
/// stable generic vendor/driver label and therefore becomes its qualifier.
/// The native driver (`xe`, `amdgpu`, `nvidia`, …) is deliberately absent from
/// this projection: it remains a separate technical fact in device details.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuDisplayIdentity<'a> {
    pub headline: Option<&'a str>,
    pub qualifier: Option<&'a str>,
}

/// Select the most specific honest GPU identity without inventing a product
/// name when native identity resolution did not provide one.
#[must_use]
pub fn gpu_display_identity(gpu: &GpuMetrics) -> GpuDisplayIdentity<'_> {
    let marketing_name = gpu
        .marketing_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let brand = (!gpu.brand.trim().is_empty()).then(|| gpu.brand.trim());

    match (marketing_name, brand) {
        (Some(headline), Some(brand)) if !headline.eq_ignore_ascii_case(brand) => {
            GpuDisplayIdentity {
                headline: Some(headline),
                qualifier: Some(brand),
            }
        }
        (Some(headline), _) => GpuDisplayIdentity {
            headline: Some(headline),
            qualifier: None,
        },
        (None, Some(headline)) => GpuDisplayIdentity {
            headline: Some(headline),
            qualifier: None,
        },
        (None, None) => GpuDisplayIdentity::default(),
    }
}

/// Format a temperature in °C with one decimal — the health-page convention
/// (a per-surface display choice, deliberately more precise than the
/// badge/graph [`temperature_c`]).
#[must_use]
pub fn temperature_c_precise(value: f32) -> String {
    format!("{value:.1} °C")
}

/// Format one SMBIOS memory inventory snapshot into canonical (label, value) rows:
/// first the slots used/total summary, then each populated physical DIMM module.
#[must_use]
pub fn smbios_memory_inventory_rows(
    snapshot: &SmbiosMemorySnapshot,
    units: UnitPreferences,
) -> Vec<(String, String)> {
    if snapshot.failure.is_some() {
        return Vec::new();
    }
    let mut rows = vec![(
        i18n::t("system.memory_slots").to_string(),
        format!(
            "{} / {} {}",
            snapshot.slots_used,
            snapshot.slots_total,
            i18n::t("common.used")
        ),
    )];
    for module in &snapshot.modules {
        let label = module
            .locator
            .clone()
            .filter(|loc| !loc.trim().is_empty())
            .unwrap_or_else(|| format!("{} {}", i18n::t("system.slot"), module.slot));
        let mut parts: Vec<String> = Vec::new();
        if let Some(part) = &module.part_number {
            let part = part.trim();
            if !part.is_empty() {
                parts.push(part.to_string());
            }
        }
        if let Some(mb) = module.size_mb {
            parts.push(units.format_quantity(
                u64::from(mb) * 1024 * 1024,
                QuantityFamily::Memory,
                false,
            ));
        }
        if let Some(speed) = module.configured_speed_mts.or(module.speed_mts) {
            parts.push(format!("{speed} MT/s"));
        }
        if let Some(mfg) = &module.manufacturer {
            let mfg = mfg.trim();
            if !mfg.is_empty() {
                parts.push(mfg.to_string());
            }
        }
        if let Some(ff) = &module.form_factor {
            let ff = ff.trim();
            if !ff.is_empty() {
                parts.push(ff.to_string());
            }
        }
        if let Some(mem_type) = &module.memory_type {
            let mem_type = mem_type.trim();
            if !mem_type.is_empty() {
                parts.push(mem_type.to_string());
            }
        }
        let value = if parts.is_empty() {
            missing_value()
        } else {
            parts.join(" · ")
        };
        rows.push((label, value));
    }
    rows
}

/// Format a fan speed in RPM the way every badge/graph readout does
/// (rounded to a whole number). The SINGLE RPM spelling for `f32` samples.
#[must_use]
pub fn fan_rpm(value: f32) -> String {
    format!("{value:.0} RPM")
}

/// Format an integral fan speed (`u32` sensor value) as a bare integer +
/// `RPM` — the health-page convention, which shows the exact reading rather
/// than a rounded one.
#[must_use]
pub fn fan_rpm_i(value: u32) -> String {
    format!("{value} RPM")
}

/// Format a power draw in watts with one decimal — the badge/graph
/// convention. The SINGLE W spelling for `f32` samples.
#[must_use]
pub fn power_w(value: f32) -> String {
    format!("{value:.1} W")
}

/// Format a power draw in watts with two decimals — the health-page
/// convention (a per-surface display choice, more precise than
/// [`power_w`]).
#[must_use]
pub fn power_w_precise(value: f32) -> String {
    format!("{value:.2} W")
}

/// Format a clock frequency in MHz (whole numbers) — the badge/graph
/// convention. The SINGLE MHz spelling.
#[must_use]
pub fn megahertz(value: f32) -> String {
    format!("{value:.0} MHz")
}

/// Format a CPU clock in GHz with the product's two-decimal precision.
///
/// The SINGLE GHz spelling. A clock of 3280 MHz is `3.28 GHz`, never the
/// rounded `3.3 GHz` a locally chosen `{:.1}` would print: two frontends
/// reading the same observation must never show different numbers.
#[must_use]
pub fn gigahertz(megahertz_value: f64) -> String {
    format!("{:.2} GHz", megahertz_value / 1000.0)
}

/// Sidebar CPU caption ladder: at or above 1 GHz the shared GHz spelling,
/// below it the shared MHz spelling. An unobserved clock stays absent.
#[must_use]
pub fn cpu_clock_readout(mhz: u64) -> String {
    if mhz >= 1000 {
        gigahertz(mhz as f64)
    } else {
        megahertz(mhz as f32)
    }
}

/// Map a Wi-Fi signal level in dBm onto the 0..=100 quality percentage every
/// frontend renders: `-90 dBm → 0%`, `-30 dBm → 100%`, anything outside that
/// window clamped. This is the SINGLE mapping (ADR-020) behind the sidebar
/// rail caption, the network detail summary, and any signal progress bar, so
/// two surfaces can never disagree about the same radio.
#[must_use]
pub fn wifi_signal_quality_percent(dbm: i32) -> f32 {
    ((dbm as f32 + 90.0) / 60.0 * 100.0).clamp(0.0, 100.0)
}

/// [`wifi_signal_quality_percent`] over an optional dBm observation: a
/// missing reading stays missing instead of becoming a fabricated `0%`.
#[must_use]
pub fn optional_wifi_signal_quality_percent(dbm: Option<i32>) -> Option<f32> {
    dbm.map(wifi_signal_quality_percent)
}

/// Finite statistics for one renderer-facing history graph.
///
/// Every frontend uses the same reduction: provider gaps are ignored, the
/// latest field is the newest finite sample (not necessarily the last array
/// element), and an all-gap/empty window returns `None`. Keeping this in the
/// shell prevents the TUI, Iced, and GPUI summaries from disagreeing about
/// what a graph's "latest", "average", or "peak" means.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphSummary {
    pub latest: f32,
    pub average: f32,
    pub minimum: f32,
    pub maximum: f32,
    pub sample_count: usize,
}

/// Reduce one graph sample window without turning provider gaps into values.
#[must_use]
pub fn graph_summary(samples: &[f32]) -> Option<GraphSummary> {
    let mut latest = None;
    let mut sum = 0.0_f32;
    let mut count = 0_usize;
    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;

    for &value in samples {
        if !value.is_finite() {
            continue;
        }
        latest = Some(value);
        sum += value;
        count = count.saturating_add(1);
        minimum = minimum.min(value);
        maximum = maximum.max(value);
    }

    let latest = latest?;
    let average = sum / count as f32;
    if !average.is_finite() {
        return None;
    }
    Some(GraphSummary {
        latest,
        average,
        minimum,
        maximum,
        sample_count: count,
    })
}

/// Peak over a finite-sample window, floored by the optional current
/// observation (a series with no history still reports the live reading).
/// Returns `None` when neither the window nor the current value holds data —
/// the honest absence callers render as a dash, never a fabricated zero.
/// This is the SINGLE peak fold (ADR-020): the GPUI Properties dialog and the
/// TUI Properties modal must not disagree about what "peak" means.
#[must_use]
pub fn peak_of(samples: &[f32], current: Option<f32>) -> Option<f32> {
    let mut peak = current.map(|value| value.max(0.0));
    for &sample in samples {
        if sample.is_finite() {
            peak = Some(peak.map_or(sample, |known| known.max(sample)));
        }
    }
    peak.map(|value| value.max(0.0))
}

/// Localized `"{value} (peak {peak})"` join for the process Properties
/// performance readouts. Either side may be missing: a missing peak renders
/// the bare value, a missing value with a known peak keeps the dash, and no
/// data at all renders the shared dash — never `0.0` invented from `None`.
#[must_use]
pub fn value_with_peak(value: Option<String>, peak: Option<String>) -> String {
    match (&value, &peak) {
        (None, None) => missing_value(),
        (Some(value), None) => value.clone(),
        _ => i18n::t("common.value_with_peak")
            .replace("{value}", value.as_deref().unwrap_or(MISSING_VALUE))
            .replace("{peak}", peak.as_deref().unwrap_or(MISSING_VALUE)),
    }
}

/// Percent-encode a string for a URL query component: RFC 3986 unreserved
/// characters pass through, every other byte becomes `%XX`. Pure (no native
/// command) so it lives here in the shell; the frontend builds a search URL and
/// emits `PlatformEffect::OpenUrl`, and the platform adapter owns the spawn.
#[must_use]
pub fn url_encode_query(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                output.push(char::from(byte));
            }
            _ => {
                output.push('%');
                output.push_str(&format!("{byte:02X}"));
            }
        }
    }
    output
}

/// The web-search URL for a process name (Google), query-percent-encoded so a
/// multi-word name does not break the URL.
#[must_use]
pub fn search_url_for(name: &str) -> String {
    format!("https://www.google.com/search?q={}", url_encode_query(name))
}

#[cfg(test)]
#[path = "../tests/headless/shell_presentation.rs"]
mod tests;
