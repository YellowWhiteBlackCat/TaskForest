//! Intel iGPU PMU discovery — pure safe `/sys` reads. Finds the integrated
//! Intel GPU, resolves its perf PMU (`xe_<BDF>` for the `xe` driver, the global
//! `i915` for the `i915` driver), and enumerates the per-engine busy configs.
//!
//! This is ported minimal from `crates/taskmanager-platform-linux`'s
//! `intel/pmu.rs` (whose discovery fns are `pub(crate)`, unreachable across
//! crates). The privileged helper keeps its own small, auditable discovery
//! surface rather than pulling the whole Linux adapter — minimal privileged
//! attack surface. Everything here is `#![forbid(unsafe_code)]`; the perf
//! syscall itself happens later in `crate::sample` through the audited
//! boundary crate's safe API.
//!
//! # Config encodings
//!
//! i915 (matches `intel_gpu_top` / kernel `i915_pmu.c`; a counter read returns
//! cumulative busy NANoseconds — rate over wall-elapsed):
//! ```text
//! config = (engine_class << 12) | (engine_instance << 4) | I915_SAMPLE_BUSY(=0)
//! ```
//!
//! xe (kernel `drivers/gpu/drm/xe/xe_pmu.c`; TWO counters per engine —
//! `XE_PMU_EVENT_ENGINE_ACTIVE_TICKS = 0x02` and `..._TOTAL_TICKS = 0x03`; a
//! read returns cumulative TICKS — rate = `active_delta / total_delta`):
//! ```text
//! config = (event & 0xFFF) << event_shift
//!        | (engine_instance & 0xFF) << instance_shift
//!        | (engine_class    & 0xFF) << class_shift
//! ```
//! with `function = 0` and `gt = 0`. The shifts are read defensively from
//! `<pmu>/format/{event,engine_instance,engine_class}`, falling back to the
//! kernel defaults (`0`/`12`/`20`) when a format file is absent — pinning the
//! layout against future kernel drift without hard-coding.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine_names;

/// `/sys/class/drm` — scanned for the Intel `cardN` device.
const DRM_CLASS: &str = "/sys/class/drm";
/// `/sys/bus/event_source/devices` — scanned for the `xe_<BDF>` / `i915` PMU.
const EVENT_SOURCE_DEVICES: &str = "/sys/bus/event_source/devices";

/// i915 PMU sample id for cumulative engine busy time.
const I915_SAMPLE_BUSY: u64 = 0;
/// xe PMU event id for cumulative engine ACTIVE ticks (the "busy" numerator).
const XE_EVENT_ACTIVE_TICKS: u64 = 0x02;
/// xe PMU event id for cumulative engine TOTAL ticks (the elapsed denominator).
const XE_EVENT_TOTAL_TICKS: u64 = 0x03;

/// Default xe config bitfield shifts, straight from `xe_pmu.c`. Used only when a
/// `<pmu>/format/<name>` file is absent; parsed shifts otherwise win.
const XE_DEFAULT_EVENT_SHIFT: u32 = 0;
const XE_DEFAULT_INSTANCE_SHIFT: u32 = 12;
const XE_DEFAULT_CLASS_SHIFT: u32 = 20;

/// The Intel GPU driver in use on this host. Drives the PMU resolution path and
/// becomes the JSON `driver` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// `xe` — Intel Core Ultra / Xe-LPG. Per-device `xe_<BDF>` PMU; two-counter
    /// ticks rate math.
    Xe,
    /// `i915` — legacy. Global `i915` PMU; cumulative-busy-ns rate math.
    I915,
}

impl Driver {
    /// The JSON `driver` keyword.
    pub const fn keyword(self) -> &'static str {
        match self {
            Driver::Xe => "xe",
            Driver::I915 => "i915",
        }
    }

    /// Parse the `DRIVER=` value from a DRM device `uevent`.
    fn from_uevent_driver(value: &str) -> Option<Self> {
        match value.trim() {
            "xe" => Some(Driver::Xe),
            "i915" => Some(Driver::I915),
            _ => None,
        }
    }
}

/// One discovered xe engine: BOTH the active-ticks and total-ticks perf configs
/// (xe returns cumulative TICKS, so a single config cannot form a busy ratio).
#[derive(Debug, Clone)]
pub struct XeEngineCfg {
    pub label: String,
    /// Numeric class id (UAPI). Read by the unit tests and useful for on-box
    /// diagnostics; not consumed by the emit path, which uses `class_name`.
    #[allow(dead_code)]
    pub class: u32,
    pub class_name: String,
    /// Instance digit (0 for xe system-wide per-class counters). Diagnostic.
    #[allow(dead_code)]
    pub instance: u32,
    pub active_config: u64,
    pub total_config: u64,
}

/// One discovered i915 engine: one cumulative-busy-ns config.
#[derive(Debug, Clone)]
pub struct I915EngineCfg {
    pub label: String,
    /// Numeric class id (UAPI). Read by the unit tests; not consumed by emit.
    #[allow(dead_code)]
    pub class: u32,
    pub class_name: String,
    /// Instance digit (i915 addresses counters per class+instance). Diagnostic.
    #[allow(dead_code)]
    pub instance: u32,
    pub config: u64,
}

/// The discovered PMU layout: its `type` id plus a busy config per engine.
#[derive(Debug, Clone)]
pub enum PmuLayout {
    Xe {
        pmu_type: u32,
        cpu: i32,
        engines: Vec<XeEngineCfg>,
    },
    I915 {
        pmu_type: u32,
        cpu: i32,
        engines: Vec<I915EngineCfg>,
    },
}

/// Find the integrated Intel GPU under the real `/sys/class/drm`. Returns its
/// `cardN/device` path and driver, or `None` when no `xe`/`i915` card is present
/// (the honest answer — never a guess at a non-Intel card).
pub fn discover_intel_gpu() -> Option<(PathBuf, Driver)> {
    discover_intel_gpu_in(Path::new(DRM_CLASS))
}

/// Testable core of [`discover_intel_gpu`] — takes the `/sys/class/drm` root.
///
/// Scans `card[0-9]*` in numeric order, returns the first whose `device/uevent`
/// advertises `DRIVER=xe` or `DRIVER=i915`. The integrated GPU is normally the
/// lowest-numbered card, so first-match is correct on single-Intel-GPU hosts;
/// on a dual-GPU box the discrete card carries a different driver and is
/// skipped naturally.
pub fn discover_intel_gpu_in(drm_root: &Path) -> Option<(PathBuf, Driver)> {
    let mut cards = match read_dir_sorted(drm_root) {
        Ok(entries) => entries,
        Err(_) => return None,
    };
    // Numeric card order: card0 before card10. Directory names are `card<N>`.
    cards.sort_by_key(|name| numeric_card_rank(name));
    for entry in cards {
        let path = drm_root.join(&entry);
        if !path.is_dir() {
            continue;
        }
        let device = path.join("device");
        let Some(driver) = read_uevent_driver(&device) else {
            continue;
        };
        if let Some(driver) = Driver::from_uevent_driver(&driver) {
            return Some((device, driver));
        }
    }
    None
}

/// Resolve the PMU layout for `(device, driver)` against the real
/// `/sys/bus/event_source/devices`. Returns `None` (no fabrication) when no PMU
/// matches or no engine could be parsed — the caller then emits `no_pmu`.
pub fn discover_pmu_layout(device: &Path, driver: Driver) -> Option<PmuLayout> {
    discover_pmu_layout_in(device, driver, Path::new(EVENT_SOURCE_DEVICES))
}

/// Testable core of [`discover_pmu_layout`] — takes the event-source root.
pub fn discover_pmu_layout_in(
    device: &Path,
    driver: Driver,
    event_source: &Path,
) -> Option<PmuLayout> {
    match driver {
        Driver::Xe => {
            let pmu_dir = resolve_xe_pmu_dir(device, event_source)?;
            let pmu_type = read_u32_from(&pmu_dir.join("type"))?;
            let cpu = read_cpumask_cpu(&pmu_dir);
            let layout = parse_xe_config_layout(&pmu_dir);
            let engines = discover_xe_engine_configs(device, &layout);
            if engines.is_empty() {
                return None;
            }
            Some(PmuLayout::Xe {
                pmu_type,
                cpu,
                engines,
            })
        }
        Driver::I915 => {
            let i915_dir = event_source.join("i915");
            let pmu_type = read_u32_from(&i915_dir.join("type"))?;
            let cpu = read_cpumask_cpu(&i915_dir);
            let engines = discover_i915_engine_configs(device);
            if engines.is_empty() {
                return None;
            }
            Some(PmuLayout::I915 {
                pmu_type,
                cpu,
                engines,
            })
        }
    }
}

/// Read the first CPU from a PMU's `cpumask` sysfs file (e.g. `0`, `0-3`).
///
/// An uncore PMU like i915/xe is pinned to one CPU, and `perf_event_open(2)`
/// rejects `pid == -1 && cpu == -1` with `EINVAL` — so the caller MUST pass this
/// cpu. Defaults to `0` (the common single-CPU uncore value) when the file is
/// absent or unparseable, rather than blocking PMU open on a missing field.
fn read_cpumask_cpu(pmu_dir: &Path) -> i32 {
    let Ok(raw) = fs::read_to_string(pmu_dir.join("cpumask")) else {
        return 0;
    };
    let digits: String = raw
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<i32>().unwrap_or(0)
}

// --- xe PMU resolution --------------------------------------------------------

/// Resolve the `xe_<BDF>` PMU directory for one DRM device.
///
/// Prefers the PCI-slot-matched device (`xe_0000_00_02.0` for slot
/// `0000:00:02.0`); falls back to the lone `xe_*` device on single-Intel-GPU
/// hosts; returns `None` when several unmatched `xe_*` devices exist (the
/// honest answer rather than a wrong-card guess).
fn resolve_xe_pmu_dir(device: &Path, event_source: &Path) -> Option<PathBuf> {
    if let Some(slot) = read_pci_slot(device) {
        let expected = format!("xe_{}", slot.replace(':', "_"));
        let candidate = event_source.join(&expected);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    let entries = read_dir_sorted(event_source).ok()?;
    let xe_dirs: Vec<PathBuf> = entries
        .into_iter()
        .map(|name| event_source.join(name))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("xe"))
        })
        .collect();
    (xe_dirs.len() == 1).then(|| xe_dirs[0].clone())
}

/// Read the `PCI_SLOT_NAME` field of `<device>/uevent` for the BDF match.
fn read_pci_slot(device: &Path) -> Option<String> {
    let uevent = fs::read_to_string(device.join("uevent")).ok()?;
    uevent.lines().find_map(|line| {
        line.strip_prefix("PCI_SLOT_NAME=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

/// Read the `DRIVER=` field of `<device>/uevent`.
fn read_uevent_driver(device: &Path) -> Option<String> {
    let uevent = fs::read_to_string(device.join("uevent")).ok()?;
    uevent.lines().find_map(|line| {
        line.strip_prefix("DRIVER=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

// --- xe config shift parsing --------------------------------------------------

/// Parsed xe config bitfield low bits. `pack_engine_busy` reproduces the
/// `xe_pmu.c` encoding using ONLY the parsed shifts, so future kernel bitfield
/// drift is surfaced here rather than silently mis-addressing an engine.
#[derive(Debug, Clone, Copy)]
struct XeConfigLayout {
    event_shift: u32,
    instance_shift: u32,
    class_shift: u32,
}

impl XeConfigLayout {
    /// Build the `xe_pmu.c` engine-busy config for `(event, class, instance)`.
    /// `function = 0` and `gt = 0` for engine-busy; only event/class/instance
    /// encode. Per-field widths from `xe_pmu.c` (event 12b, instance/class 8b)
    /// so a parsed shift cannot smear a value into a neighbouring field.
    fn pack_engine_busy(&self, event: u64, class: u32, instance: u32) -> u64 {
        (event & 0xFFF) << self.event_shift
            | (u64::from(instance) & 0xFF) << self.instance_shift
            | (u64::from(class) & 0xFF) << self.class_shift
    }
}

/// Parse the three xe config bitfields that matter for engine-busy from
/// `<pmu>/format/*`, with kernel defaults for any absent file. A field whose low
/// bit fails to parse (or is `>= 64`, which would overflow a `u64` shift) falls
/// back to the default.
fn parse_xe_config_layout(pmu_dir: &Path) -> XeConfigLayout {
    XeConfigLayout {
        event_shift: parse_format_low_bit(pmu_dir, "event").unwrap_or(XE_DEFAULT_EVENT_SHIFT),
        instance_shift: parse_format_low_bit(pmu_dir, "engine_instance")
            .unwrap_or(XE_DEFAULT_INSTANCE_SHIFT),
        class_shift: parse_format_low_bit(pmu_dir, "engine_class")
            .unwrap_or(XE_DEFAULT_CLASS_SHIFT),
    }
}

/// Lowest `config` bit declared by a xe `format/<name>` file.
///
/// Lines look like `config:0-11` (optionally comma-separated for discontiguous
/// fields); we take the low end of the first range. Values `>= 64` are rejected
/// so a garbage file cannot produce a panicking shift.
fn parse_format_low_bit(pmu_dir: &Path, name: &str) -> Option<u32> {
    let raw = fs::read_to_string(pmu_dir.join("format").join(name)).ok()?;
    let after_config = raw.split(':').nth(1)?;
    let first_range = after_config.trim().split(',').next()?;
    let low = first_range.split('-').next()?.trim();
    let bit: u32 = low.parse().ok()?;
    (bit < 64).then_some(bit)
}

// --- engine enumeration -------------------------------------------------------

/// Walk the GT `tile*/gt*/engines/` tree and return one path per `gtN`.
fn discover_gt_directories(device: &Path) -> Vec<PathBuf> {
    let tiles = match read_dir_sorted(device) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut directories = Vec::new();
    for tile_name in tiles {
        if !tile_name.starts_with("tile") {
            continue;
        }
        let tile_path = device.join(&tile_name);
        let Ok(gts) = read_dir_sorted(&tile_path) else {
            continue;
        };
        for gt in gts {
            if gt.starts_with("gt") {
                directories.push(tile_path.join(gt));
            }
        }
    }
    directories.sort();
    directories
}

/// Walk the xe `tile*/gt*/engines/` tree and build an active+total config pair
/// per DISTINCT engine class. Each class collapses to instance 0 (the per-class
/// xe PMU counters are system-wide) and is de-duplicated across GTs, so a class
/// repeated under `gt0` and `gt1` yields ONE config pair, not two.
fn discover_xe_engine_configs(device: &Path, layout: &XeConfigLayout) -> Vec<XeEngineCfg> {
    let mut by_class: BTreeMap<u32, XeEngineCfg> = BTreeMap::new();
    for gt in discover_gt_directories(device) {
        let Ok(entries) = read_dir_sorted(&gt.join("engines")) else {
            // Absent `engines/` on one GT is soft: keep scanning sibling GTs.
            continue;
        };
        for name in entries {
            if name.starts_with('.') {
                // xe tucks a `.defaults` metadata kobject under engines/.
                continue;
            }
            let Some(parsed) = engine_names::parse_xe_engine(&name) else {
                continue;
            };
            by_class.entry(parsed.class).or_insert(XeEngineCfg {
                label: engine_names::engine_label(&name),
                class: parsed.class,
                class_name: engine_names::class_keyword(parsed.class).to_string(),
                instance: parsed.instance,
                active_config: layout.pack_engine_busy(
                    XE_EVENT_ACTIVE_TICKS,
                    parsed.class,
                    parsed.instance,
                ),
                total_config: layout.pack_engine_busy(
                    XE_EVENT_TOTAL_TICKS,
                    parsed.class,
                    parsed.instance,
                ),
            });
        }
    }
    by_class.into_values().collect()
}

/// Walk the i915 `tile*/gt*/engines/` tree and build one busy config per engine
/// instance. Unlike xe, i915 addresses counters per `(class, instance)`, so
/// multiple instances of one class each yield a config (no de-dup).
fn discover_i915_engine_configs(device: &Path) -> Vec<I915EngineCfg> {
    let mut engines = Vec::new();
    for gt in discover_gt_directories(device) {
        let Ok(entries) = read_dir_sorted(&gt.join("engines")) else {
            continue;
        };
        for name in entries {
            if name.starts_with('.') {
                continue;
            }
            let Some(parsed) = engine_names::parse_i915_engine(&name) else {
                continue;
            };
            let config = (u64::from(parsed.class)) << 12
                | (u64::from(parsed.instance)) << 4
                | I915_SAMPLE_BUSY;
            engines.push(I915EngineCfg {
                label: engine_names::engine_label(&name),
                class: parsed.class,
                class_name: engine_names::class_keyword(parsed.class).to_string(),
                instance: parsed.instance,
                config,
            });
        }
    }
    engines
}

// --- small safe /sys helpers --------------------------------------------------

fn read_u32_from(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse::<u32>().ok()
}

/// Read a directory's entry names, sorted lexically. Errors bubble as `Err` so
/// callers can distinguish "absent dir" from "empty dir".
fn read_dir_sorted(path: &Path) -> std::io::Result<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(path)?
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(ToOwned::to_owned))
        .collect();
    names.sort();
    Ok(names)
}

/// Sort key that orders `card0` before `card10` (strip the `card` prefix, parse
/// the rest numerically; a non-numeric tail sorts after every numeric name).
fn numeric_card_rank(name: &str) -> (u32, String) {
    match name
        .strip_prefix("card")
        .and_then(|rest| rest.parse::<u32>().ok())
    {
        Some(number) => (0, number.to_string()),
        None => (1, name.to_string()),
    }
}

#[cfg(test)]
#[path = "../tests/headless/privilege_discovery.rs"]
mod tests;
