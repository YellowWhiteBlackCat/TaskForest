//! Pure JSON, CSV and self-contained HTML snapshot formatters.

use serde::Serialize;
use serde_json::Value;
use std::path::Path;

use crate::core::alerts::SuggestedThreshold;
use crate::core::hardware::HardwareInfo;
use crate::core::metrics::{
    CpuMetrics, DiskMetrics, GpuMetrics, MemoryMetrics, NetworkMetrics, SystemSnapshot,
};
use crate::core::npu::NpuInventorySnapshot;
use crate::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessScalarObservations,
};
use crate::core::process_telemetry::{ContainerSummary, ProcessGpuEngines};

/// Owned JSON projection over a `&SystemSnapshot`.
/// Produces the same layout a `#[derive(Serialize)]` on `SystemSnapshot` would,
/// while replacing legacy numeric compatibility fields with their typed
/// current values (`null` when unavailable); see the module docs for why we do
/// not simply add that derive.
#[derive(Serialize)]
struct SnapshotJson {
    timestamp_ms: u64,
    cpu: Value,
    memory: Value,
    disks: Vec<Value>,
    networks: Vec<Value>,
    gpu: Vec<Value>,
    uptime_secs: u64,
    processes: usize,
    threads: Option<usize>,
}

fn serialized<T: Serialize>(value: &T) -> Value {
    match serde_json::to_value(value) {
        Ok(value) => value,
        // These metrics contain only serde-supported scalar/collection data.
        // Keep the export fail-closed if a future custom serializer violates
        // that invariant instead of panicking or inventing a numeric value.
        Err(_) => Value::Null,
    }
}

fn set_field(object: &mut Value, field: &str, value: Value) {
    if let Some(object) = object.as_object_mut() {
        object.insert(field.to_owned(), value);
    }
}

fn optional_value<T: Serialize>(value: Option<T>) -> Value {
    serialized(&value)
}

fn cpu_json(cpu: &CpuMetrics) -> Value {
    let mut value = serialized(cpu);
    set_field(
        &mut value,
        "global_usage",
        optional_value(cpu.current_global_usage_pct()),
    );
    let core_usage = (0..cpu.current_core_usage_len())
        .map(|index| optional_value(cpu.current_core_usage_pct(index)))
        .collect::<Vec<_>>();
    set_field(&mut value, "core_usages", serialized(&core_usage));
    set_field(
        &mut value,
        "frequency_mhz",
        optional_value(cpu.current_frequency_mhz()),
    );
    set_field(
        &mut value,
        "max_freq_mhz",
        optional_value(cpu.current_max_frequency_mhz()),
    );
    let core_frequency = (0..cpu.current_core_frequency_len())
        .map(|index| optional_value(cpu.current_core_frequency_mhz(index)))
        .collect::<Vec<_>>();
    set_field(&mut value, "per_core_freq_mhz", serialized(&core_frequency));
    set_field(
        &mut value,
        "temperature_c",
        optional_value(cpu.current_temperature_c()),
    );
    let core_temperature = (0..cpu.current_core_temperature_len())
        .map(|index| optional_value(cpu.current_core_temperature_c(index)))
        .collect::<Vec<_>>();
    set_field(
        &mut value,
        "per_core_temps_c",
        serialized(&core_temperature),
    );
    set_field(
        &mut value,
        "cpu_power_w",
        optional_value(cpu.current_power_w()),
    );
    value
}

fn memory_json(memory: &MemoryMetrics) -> Value {
    let mut value = serialized(memory);
    set_field(
        &mut value,
        "total_bytes",
        optional_value(memory.current_total_bytes()),
    );
    set_field(
        &mut value,
        "used_bytes",
        optional_value(memory.current_used_bytes()),
    );
    set_field(
        &mut value,
        "available_bytes",
        optional_value(memory.current_available_bytes()),
    );
    set_field(
        &mut value,
        "swap_total_bytes",
        optional_value(memory.current_swap_total_bytes()),
    );
    set_field(
        &mut value,
        "swap_used_bytes",
        optional_value(memory.current_swap_used_bytes()),
    );
    set_field(
        &mut value,
        "mem_used_rate_mbps",
        optional_value(memory.current_used_rate_mib_per_sec()),
    );
    value
}

fn disk_json(disk: &DiskMetrics) -> Value {
    let mut value = serialized(disk);
    set_field(
        &mut value,
        "total_bytes",
        optional_value(disk.current_capacity_bytes()),
    );
    set_field(
        &mut value,
        "available_bytes",
        optional_value(disk.current_available_bytes()),
    );
    set_field(
        &mut value,
        "read_bytes_per_sec",
        optional_value(disk.current_read_bytes_per_sec()),
    );
    set_field(
        &mut value,
        "write_bytes_per_sec",
        optional_value(disk.current_write_bytes_per_sec()),
    );
    set_field(&mut value, "iops", optional_value(disk.current_iops()));
    set_field(
        &mut value,
        "active_time_pct",
        optional_value(disk.current_active_time_pct()),
    );
    set_field(
        &mut value,
        "response_time_ms",
        optional_value(disk.current_response_time_ms()),
    );
    value
}

fn network_json(network: &NetworkMetrics) -> Value {
    let mut value = serialized(network);
    set_field(
        &mut value,
        "total_rx_bytes",
        optional_value(network.current_total_rx_bytes()),
    );
    set_field(
        &mut value,
        "total_tx_bytes",
        optional_value(network.current_total_tx_bytes()),
    );
    set_field(
        &mut value,
        "rx_bytes_per_sec",
        optional_value(network.current_rx_bytes_per_sec()),
    );
    set_field(
        &mut value,
        "tx_bytes_per_sec",
        optional_value(network.current_tx_bytes_per_sec()),
    );
    set_field(
        &mut value,
        "utilization_pct",
        optional_value(network.current_utilization_pct()),
    );
    set_field(
        &mut value,
        "wireless_bssid",
        optional_value(network.current_bssid()),
    );
    set_field(
        &mut value,
        "wireless_frequency_mhz",
        optional_value(network.current_frequency_mhz()),
    );
    set_field(
        &mut value,
        "wireless_channel",
        optional_value(network.current_channel()),
    );
    set_field(
        &mut value,
        "wireless_rx_bitrate_mbps",
        optional_value(network.current_rx_bitrate_mbps()),
    );
    set_field(
        &mut value,
        "wireless_tx_bitrate_mbps",
        optional_value(network.current_tx_bitrate_mbps()),
    );
    set_field(
        &mut value,
        "wireless_protocol",
        optional_value(network.current_protocol()),
    );
    value
}

fn gpu_json(gpu: &GpuMetrics) -> Value {
    let mut value = serialized(gpu);
    set_field(
        &mut value,
        "gpu_usage_pct",
        optional_value(gpu.current_utilization_pct()),
    );
    set_field(
        &mut value,
        "utilization_pct",
        optional_value(gpu.current_utilization_pct()),
    );
    set_field(
        &mut value,
        "temp_celsius",
        optional_value(gpu.current_temperature_c()),
    );
    set_field(
        &mut value,
        "temperature_c",
        optional_value(gpu.current_temperature_c()),
    );
    set_field(
        &mut value,
        "vram_used_bytes",
        optional_value(gpu.current_dedicated_vram_used_bytes()),
    );
    set_field(
        &mut value,
        "vram_total_bytes",
        optional_value(gpu.current_dedicated_vram_total_bytes()),
    );
    set_field(
        &mut value,
        "dedicated_vram_used_bytes",
        optional_value(gpu.current_dedicated_vram_used_bytes()),
    );
    set_field(
        &mut value,
        "dedicated_vram_total_bytes",
        optional_value(gpu.current_dedicated_vram_total_bytes()),
    );
    set_field(
        &mut value,
        "shared_vram_used_bytes",
        optional_value(gpu.current_shared_vram_used_bytes()),
    );
    set_field(
        &mut value,
        "shared_vram_total_bytes",
        optional_value(gpu.current_shared_vram_total_bytes()),
    );
    set_field(
        &mut value,
        "memory_used_bytes",
        optional_value(gpu.current_memory_used_bytes()),
    );
    set_field(
        &mut value,
        "memory_total_bytes",
        optional_value(gpu.current_memory_total_bytes()),
    );
    set_field(
        &mut value,
        "gpu_freq_mhz",
        optional_value(gpu.current_frequency_mhz()),
    );
    set_field(
        &mut value,
        "max_freq_mhz",
        optional_value(gpu.current_max_frequency_mhz()),
    );
    set_field(
        &mut value,
        "fan_speed_rpm",
        optional_value(gpu.current_fan_speed_rpm()),
    );
    set_field(
        &mut value,
        "fan_speed_pct",
        optional_value(gpu.current_fan_speed_pct()),
    );
    set_field(
        &mut value,
        "gpu_power_w",
        optional_value(gpu.current_power_w()),
    );
    set_field(
        &mut value,
        "idle_residency_pct",
        optional_value(gpu.current_idle_residency_pct()),
    );
    set_field(
        &mut value,
        "rc6_idle_pct",
        optional_value(gpu.current_idle_residency_pct()),
    );
    value
}

/// Export projection for one process row. The legacy numeric fields on
/// [`ProcessItem`] use zero as a compatibility sentinel, which is not a safe
/// machine-readable value for a provider that honestly reported unavailable.
/// Export therefore serializes current typed observations (`null` when absent)
/// while retaining the typed observation groups for consumers that need the
/// failure/staleness reason.
#[derive(Serialize)]
struct ProcessJsonRow<'a> {
    pid: u32,
    parent_pid: Option<u32>,
    name: &'a str,
    cmdline: &'a str,
    cpu_usage: Option<f32>,
    memory_bytes: Option<u64>,
    disk_read_bytes: Option<u64>,
    disk_write_bytes: Option<u64>,
    status: &'a str,
    user: Option<String>,
    exe_path: Option<String>,
    metadata_observations: &'a ProcessMetadataObservations,
    application_identity: &'a ProcessMetadataObservation<ProcessApplicationIdentity>,
    threads: Option<u32>,
    start_time_secs: Option<u64>,
    cpu_time_secs: Option<u64>,
    fds: Option<u32>,
    nice: Option<i32>,
    scalar_observations: &'a ProcessScalarObservations,
    cpu_history: &'a [f32],
    mem_history: &'a [f32],
    disk_history: &'a [f32],
    disk_read_history: &'a [f32],
    disk_write_history: &'a [f32],
}

fn process_json_row(process: &ProcessItem) -> ProcessJsonRow<'_> {
    ProcessJsonRow {
        pid: process.pid,
        parent_pid: process.parent_pid,
        name: &process.name,
        cmdline: &process.cmdline,
        cpu_usage: process.current_cpu_percentage(),
        memory_bytes: process.current_memory_bytes(),
        disk_read_bytes: process.current_disk_read_bytes_per_sec(),
        disk_write_bytes: process.current_disk_write_bytes_per_sec(),
        status: &process.status,
        user: process.current_user(),
        exe_path: process
            .current_exe_path()
            .map(|path: &Path| path.to_string_lossy().into_owned()),
        metadata_observations: process.metadata_observations(),
        application_identity: process.application_identity_observation(),
        threads: process.current_threads(),
        start_time_secs: process.current_start_time_secs(),
        cpu_time_secs: process.current_cpu_time_secs(),
        fds: process.current_fds(),
        nice: process.current_nice(),
        scalar_observations: process.scalar_observations(),
        cpu_history: &process.cpu_history,
        mem_history: &process.mem_history,
        disk_history: &process.disk_history,
        disk_read_history: &process.disk_read_history,
        disk_write_history: &process.disk_write_history,
    }
}

/// One per-process GPU engine breakdown in the export envelope. Pairs a process
/// id with its typed [`ProcessGpuEngines`] so a JSON reader gets a stable
/// `{"pid": ..., "engines": {...}}` object instead of a positional tuple.
///
/// The engines view is borrowed (zero-copy): the entry is only constructed by a
/// caller that already holds a live [`ProcessGpuEngines`] for the pid.
#[derive(Serialize)]
pub struct ProcessGpuEnginesEntry<'a> {
    /// Process id this breakdown belongs to.
    pub pid: u32,
    /// Typed per-engine utilization plus the collection health of the
    /// `/proc/<pid>/fd` + `fdinfo` scan that produced it.
    pub engines: &'a ProcessGpuEngines,
}

/// Optional side-channel telemetry the export envelope can carry but which
/// [`SystemSnapshot`] and [`ProcessItem`] do not own directly: the container
/// cgroup rollup, per-process GPU engine breakdowns, and alert threshold
/// suggestions.
///
/// Every field defaults to an empty slice, so a caller that has none of this
/// data passes [`ExportExtras::default()`] and the envelope emits honest empty
/// arrays — never fabricated rows or thresholds. Callers that do hold the data
/// (a future streaming CLI, the GUI process-insights panel) pass real slices,
/// and the envelope serializes them additively alongside the existing snapshot
/// and process list. The shape is intentionally backward-compatible: the three
/// new top-level keys are always present, as `[]` when no extras are supplied.
#[derive(Default)]
pub struct ExportExtras<'a> {
    /// Aggregated container cgroup summaries, in collector order. Empty when
    /// the host has no cgroup-v2 containers or the rollup source is typed
    /// unavailable.
    pub containers: &'a [ContainerSummary],
    /// Per-process GPU engine breakdowns, one entry per observed pid. Empty
    /// when no process held DRM descriptors, or when the caller did not collect
    /// them for this export.
    pub process_gpu_engines: &'a [ProcessGpuEnginesEntry<'a>],
    /// Alert threshold suggestions, one per metric the caller evaluated. An
    /// `Insufficient` entry is serialized verbatim (typed reason) rather than
    /// as a fabricated threshold value.
    pub suggested_thresholds: &'a [SuggestedThreshold],
    /// Static hardware facts (brand, topology, instruction features, ...).
    /// `None` when the caller did not collect a hardware-inventory snapshot;
    /// the envelope then omits the key entirely rather than emitting a
    /// fabricated empty block.
    pub hardware: Option<&'a HardwareInfo>,
    /// NPU accelerator inventory (capability `accelerator.npu`): a sorted
    /// device list (empty = honest no-NPU host) or a typed failure. `None`
    /// when the caller did not submit an inventory read; the key is omitted.
    pub npu_inventory: Option<&'a NpuInventorySnapshot>,
}

/// Top-level JSON envelope: the full snapshot, the (possibly empty) process
/// list, and the optional side-channel telemetry from [`ExportExtras`]. The
/// three extras arrays are always present (as `[]` when empty) so the JSON
/// shape stays stable and additive across callers.
#[derive(Serialize)]
struct ExportPayload<'a> {
    snapshot: SnapshotJson,
    processes: &'a [ProcessJsonRow<'a>],
    containers: &'a [ContainerSummary],
    process_gpu_engines: &'a [ProcessGpuEnginesEntry<'a>],
    suggested_thresholds: &'a [SuggestedThreshold],
    #[serde(skip_serializing_if = "Option::is_none")]
    hardware: Option<&'a HardwareInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    npu_inventory: Option<&'a NpuInventorySnapshot>,
}

/// Pretty-print the snapshot + process list as JSON (via `serde_json`).
///
/// Equivalent to [`snapshot_to_json_with_extras`] with an empty
/// [`ExportExtras`] — the three side-channel arrays (`containers`,
/// `process_gpu_engines`, `suggested_thresholds`) serialize as `[]`. Kept as a
/// thin wrapper so existing callers (the GUI diagnostic bundle, the
/// transactional file export) keep their signature and gain the additive keys
/// for free.
///
/// `procs` may be empty — the `processes` array then serializes as `[]`. This
/// keeps the shape stable for callers that only have the snapshot in hand.
pub fn snapshot_to_json(snap: &SystemSnapshot, procs: &[ProcessItem]) -> String {
    snapshot_to_json_with_extras(snap, procs, ExportExtras::default())
}

/// Pretty-print the snapshot + process list + side-channel telemetry as JSON.
///
/// Use this richer entry point when the caller also holds container rollups,
/// per-process GPU engine breakdowns, or alert threshold suggestions that the
/// snapshot/process types do not carry. Everything in `extras` is borrowed and
/// serialized additively; pass [`ExportExtras::default()`] (or call
/// [`snapshot_to_json`]) to emit the three extras keys as honest empty arrays.
///
/// `to_string_pretty` never fails for these types (no map keys; the suggestion
/// heuristic clamps thresholds to finite; no NaN is produced by the Serialize
/// impls), so the `expect` is safe.
pub fn snapshot_to_json_with_extras(
    snap: &SystemSnapshot,
    procs: &[ProcessItem],
    extras: ExportExtras<'_>,
) -> String {
    let process_rows: Vec<ProcessJsonRow<'_>> = procs.iter().map(process_json_row).collect();
    let payload = ExportPayload {
        snapshot: SnapshotJson {
            timestamp_ms: snap.timestamp_ms,
            cpu: cpu_json(&snap.cpu),
            memory: memory_json(&snap.memory),
            disks: snap.disks.iter().map(disk_json).collect(),
            networks: snap.networks.iter().map(network_json).collect(),
            gpu: snap.gpu.iter().map(gpu_json).collect(),
            uptime_secs: snap.uptime_secs,
            processes: snap.processes,
            threads: snap.threads,
        },
        processes: &process_rows,
        containers: extras.containers,
        process_gpu_engines: extras.process_gpu_engines,
        suggested_thresholds: extras.suggested_thresholds,
        hardware: extras.hardware,
        npu_inventory: extras.npu_inventory,
    };
    serde_json::to_string_pretty(&payload).expect("snapshot serialization is infallible")
}

/// Escape one CSV field per RFC 4180: if it contains a comma, double-quote, or
/// any newline (CR/LF), wrap it in double quotes and double every internal
/// double-quote. Otherwise emit it verbatim.
fn csv_escape(field: &str, out: &mut String) {
    let needs_quoting = field
        .as_bytes()
        .iter()
        .any(|&b| b == b',' || b == b'"' || b == b'\n' || b == b'\r');
    if !needs_quoting {
        out.push_str(field);
        return;
    }
    out.push('"');
    for &b in field.as_bytes() {
        if b == b'"' {
            // Double the quote per RFC 4180 §2.7.
            out.push('"');
            out.push('"');
        } else {
            out.push(b as char);
        }
    }
    out.push('"');
}

/// Hand-rolled CSV of the user-visible process columns. One header row then one
/// row per process, in the order given. Columns:
/// `Name,PID,CPU%,Memory MB,User,Status,Threads,Disk R,Disk W`
///
/// `Memory MB` is `memory_bytes / 1024²` to two decimals. `Disk R` / `Disk W`
/// are the per-tick throughput totals in bytes (`disk_read_bytes` /
/// `disk_write_bytes`). Name / User / Status are RFC 4180-escaped.
pub fn processes_to_csv(procs: &[ProcessItem]) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mut out = String::with_capacity(procs.len() * 64 + 128);
    out.push_str("Name,PID,CPU%,Memory MB,User,Status,Threads,Disk R,Disk W\n");
    for p in procs {
        csv_escape(&p.name, &mut out);
        out.push(',');
        out.push_str(&p.pid.to_string());
        out.push(',');
        csv_f32(p.current_cpu_percentage(), &mut out);
        out.push(',');
        csv_mib(p.current_memory_bytes(), MB, &mut out);
        out.push(',');
        let user = p.current_user();
        csv_escape(user.as_deref().unwrap_or("—"), &mut out);
        out.push(',');
        csv_escape(&p.status, &mut out);
        out.push(',');
        csv_optional(p.current_threads(), &mut out);
        out.push(',');
        csv_optional(p.current_disk_read_bytes_per_sec(), &mut out);
        out.push(',');
        csv_optional(p.current_disk_write_bytes_per_sec(), &mut out);
        out.push('\n');
    }
    out
}

fn csv_optional<T: std::fmt::Display>(value: Option<T>, out: &mut String) {
    match value {
        Some(value) => out.push_str(&value.to_string()),
        None => out.push('—'),
    }
}

fn csv_f32(value: Option<f32>, out: &mut String) {
    match value {
        Some(value) if value.is_finite() => out.push_str(&format!("{value:.2}")),
        _ => out.push('—'),
    }
}

fn csv_mib(value: Option<u64>, mb: f64, out: &mut String) {
    match value {
        Some(value) => out.push_str(&format!("{:.2}", value as f64 / mb)),
        None => out.push('—'),
    }
}

/// Escape the five HTML-significant characters (`& < > " '`) per the OWASP
/// recommended encoding so a process name / user field can never inject markup.
/// Iterating by `char` (not byte) preserves multi-byte UTF-8 unchanged. Pure
/// (no I/O); unit-tested.
pub(super) fn html_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

/// Format a byte count as mebibytes with one decimal, e.g. `1536 MiB`. Pure
/// helper for the HTML stats block; never used on the hot path.
fn fmt_mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

/// Self-contained HTML snapshot: a styled page with a key-system-stats table
/// and the user-visible process table. Every text field is HTML-escaped via
/// `html_escape` so a hostile process name (e.g. `<script>…`) renders as inert
/// text, never executable markup. No external resources (the stylesheet is
/// inlined) so the file is fully viewable offline / from disk. `procs` may be
/// empty — the table then has a header row + a "no processes" placeholder row.
///
/// Columns mirror [`processes_to_csv`]: Name, PID, CPU%, Memory, User, Status,
/// Threads, Disk R, Disk W (plus CPU time in seconds when populated).
pub fn processes_to_html(snap: &SystemSnapshot, procs: &[ProcessItem]) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mut out = String::with_capacity(procs.len() * 200 + 2048);
    // ── document head + inlined stylesheet ────────────────────────────────
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>Task Manager snapshot</title>\n<style>\n");
    out.push_str(
        "body{font-family:system-ui,Segoe UI,Roboto,sans-serif;margin:1.5rem;color:#222;}\n",
    );
    out.push_str("h1{font-size:1.2rem;margin:0 0 1rem;}\n");
    out.push_str("h2{font-size:1rem;margin:1.5rem 0 .5rem;}\n");
    out.push_str("table{border-collapse:collapse;width:100%;font-size:.85rem;}\n");
    out.push_str("th,td{border:1px solid #ddd;padding:4px 8px;text-align:left;}\n");
    out.push_str("th{background:#f4f4f4;}\n");
    out.push_str("td.num{text-align:right;font-variant-numeric:tabular-nums;}\n");
    out.push_str("caption{caption-side:top;text-align:left;color:#666;font-size:.8rem;padding-bottom:.25rem;}\n");
    out.push_str("</style>\n</head>\n<body>\n");
    out.push_str("<h1>Task Manager snapshot</h1>\n");

    // ── system stats block ────────────────────────────────────────────────
    out.push_str("<h2>System</h2>\n<table>\n");
    // CPU: brand + global utilization %. Brand is user/sysinfo-controlled text
    // → escaped.
    out.push_str("<tr><th>CPU</th><td>");
    html_escape(snap.cpu.brand.as_deref().unwrap_or("—"), &mut out);
    out.push_str(" &middot; ");
    match snap.cpu.current_global_usage_pct() {
        Some(usage) => out.push_str(&format!("{usage:.1}% global")),
        None => out.push_str("— global"),
    }
    out.push_str("</td></tr>\n");
    // Memory: used / total as MiB.
    out.push_str("<tr><th>Memory</th><td>");
    match (
        snap.memory.current_used_bytes(),
        snap.memory.current_total_bytes(),
        snap.memory.used_percentage_observed(),
    ) {
        (Some(used), Some(total), Some(percentage)) => out.push_str(&format!(
            "{} / {} ({percentage:.0}%)",
            fmt_mib(used),
            fmt_mib(total),
        )),
        (Some(used), _, _) => out.push_str(&format!("{} / — (—)", fmt_mib(used))),
        _ => out.push_str("— / — (—)"),
    }
    out.push_str("</td></tr>\n");
    // A measured zero means no swap is configured and remains hidden. Typed
    // unavailability is rendered explicitly instead of being confused with
    // that legitimate zero.
    match snap.memory.current_swap_total_bytes() {
        Some(0) => {}
        Some(total) => {
            out.push_str("<tr><th>Swap</th><td>");
            match (
                snap.memory.current_swap_used_bytes(),
                snap.memory.swap_percentage_observed(),
            ) {
                (Some(used), Some(percentage)) => out.push_str(&format!(
                    "{} / {} ({percentage:.0}%)",
                    fmt_mib(used),
                    fmt_mib(total),
                )),
                _ => out.push_str(&format!("— / {} (—)", fmt_mib(total))),
            }
            out.push_str("</td></tr>\n");
        }
        None => out.push_str("<tr><th>Swap</th><td>— / — (—)</td></tr>\n"),
    }
    // Uptime + counts.
    out.push_str("<tr><th>Uptime</th><td>");
    out.push_str(&format!("{}s", snap.uptime_secs));
    out.push_str("</td></tr>\n");
    out.push_str("<tr><th>Processes</th><td>");
    out.push_str(&snap.processes.to_string());
    out.push_str("</td></tr>\n");
    out.push_str("<tr><th>Threads</th><td>");
    out.push_str(
        &snap
            .threads
            .map_or_else(|| "—".to_owned(), |threads| threads.to_string()),
    );
    out.push_str("</td></tr>\n");
    out.push_str("</table>\n");

    // ── process table ─────────────────────────────────────────────────────
    out.push_str("<h2>Processes</h2>\n<table>\n");
    out.push_str("<caption>");
    out.push_str(&procs.len().to_string());
    out.push_str(" process(es)</caption>\n");
    out.push_str("<thead><tr><th>Name</th><th>PID</th><th>CPU%</th><th>Memory</th>");
    out.push_str("<th>User</th><th>Status</th><th>Threads</th><th>Disk R</th><th>Disk W</th>");
    out.push_str("<th>CPU time</th>");
    out.push_str("</tr></thead>\n<tbody>\n");
    if procs.is_empty() {
        // Single placeholder row so the table is never header-only (clearer
        // signal that the list was empty, not a rendering failure). colspan
        // covers all 10 columns.
        out.push_str("<tr><td colspan=\"10\"><em>No processes in this snapshot.</em></td></tr>\n");
    } else {
        for p in procs {
            let user = p.current_user();
            out.push_str("<tr><td>");
            html_escape(&p.name, &mut out);
            out.push_str("</td><td class=\"num\">");
            out.push_str(&p.pid.to_string());
            out.push_str("</td><td class=\"num\">");
            match p.current_cpu_percentage() {
                Some(value) if value.is_finite() => out.push_str(&format!("{value:.2}")),
                _ => out.push('—'),
            }
            out.push_str("</td><td class=\"num\">");
            match p.current_memory_bytes() {
                Some(value) => out.push_str(&format!("{:.2}", value as f64 / MB)),
                None => out.push('—'),
            }
            out.push_str("</td><td>");
            html_escape(user.as_deref().unwrap_or("—"), &mut out);
            out.push_str("</td><td>");
            html_escape(&p.status, &mut out);
            out.push_str("</td><td class=\"num\">");
            out.push_str(
                &p.current_threads()
                    .map_or_else(|| "—".to_owned(), |value| value.to_string()),
            );
            out.push_str("</td><td class=\"num\">");
            out.push_str(
                &p.current_disk_read_bytes_per_sec()
                    .map_or_else(|| "—".to_owned(), |value| value.to_string()),
            );
            out.push_str("</td><td class=\"num\">");
            out.push_str(
                &p.current_disk_write_bytes_per_sec()
                    .map_or_else(|| "—".to_owned(), |value| value.to_string()),
            );
            out.push_str("</td><td class=\"num\">");
            match p.current_cpu_time_secs() {
                Some(value) => out.push_str(&format!("{value}s")),
                None => out.push('—'),
            }
            out.push_str("</td></tr>\n");
        }
    }
    out.push_str("</tbody>\n</table>\n");
    out.push_str("</body>\n</html>\n");
    out
}
