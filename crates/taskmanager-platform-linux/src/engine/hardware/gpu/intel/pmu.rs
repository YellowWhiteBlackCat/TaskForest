//! Intel i915 + xe perf PMU discovery — pure safe Rust.
//!
//! This module DISCOVERS the i915 and xe `perf_event_open` PMU types and builds
//! a busy config for each engine visible in the same GT `engines/` sysfs tree
//! walked by [`super::read_intel_gt_engines`]. The actual `perf_event_open`
//! call lives in the audited boundary crate `taskmanager-perf-ioctl` (the
//! one of the workspace's four `unsafe` trust roots — see ADR-022); nothing
//! here is `unsafe`, and this crate stays `#![forbid(unsafe_code)]`.
//!
//! i915 PMU busy config encoding (matches `intel_gpu_top` / kernel
//! `i915_pmu.c`):
//! ```text
//! config = (engine_class << 12) | (engine_instance << 4) | I915_SAMPLE_BUSY
//! ```
//! with `I915_SAMPLE_BUSY = 0` and engine classes
//! `RENDER=0, COPY=1, VIDEO=2, VIDEO_ENHANCE=3, COMPUTE=4` (UAPI
//! `drm_i915_gem_engine_class`, matching the [`super::intel_engine_label`] map).
//! A successful counter read returns cumulative busy NANoseconds — the same
//! units as the sysfs `busy` node — so the existing rate-conversion path in
//! `provider/intel/engines.rs` is reused unchanged.
//!
//! # xe two-counter ticks path (phase B2)
//!
//! The `xe` driver (Intel Core Ultra / Xe-LPG) registers a SEPARATE per-device
//! PMU at `/sys/bus/event_source/devices/xe_<BDF>/` (e.g. `xe_0000_00_02.0`),
//! NOT the single global `i915` PMU. Its config bitfield layout is DIFFERENT
//! from i915 (kernel `drivers/gpu/drm/xe/xe_pmu.c`):
//! ```text
//! config = (event & 0xFFF)
//!        | (engine_instance & 0xFF) << 12
//!        | (engine_class    & 0xFF) << 20
//!        | (function         & 0xFFFF) << 44
//!        | (gt               & 0xF) << 60
//! ```
//! with event ids `XE_PMU_EVENT_ENGINE_ACTIVE_TICKS = 0x02` (busy) and
//! `XE_PMU_EVENT_ENGINE_TOTAL_TICKS = 0x03` (elapsed). For engine-busy,
//! `function = 0` and `gt = 0`. The bit shifts are read defensively from
//! `<pmu>/format/{event,engine_instance,engine_class,function,gt}` to pin the
//! layout against future kernel drift, falling back to the kernel defaults
//! when a format file is absent.
//!
//! Unlike i915, xe returns cumulative TICKS, not nanoseconds. The busy ratio is
//! `active_ticks_delta / total_ticks_delta`, computed in lockstep per engine
//! per interval by the tracker in `provider/intel/engines.rs` via the
//! `EngineBusyDelta::TickRatio` arm — it must NOT be divided by wall-clock
//! (wrong units). See [`discover_xe_pmu_layout`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use taskmanager_core::FailureKind;

use super::super::{GpuFieldRead, gpu_io_failure, preferred_gpu_failure};

/// i915 engine class ids (UAPI `drm_i915_gem_engine_class`); reused verbatim by
/// the xe driver, which shares the same class enum.
const ENGINE_CLASS_RENDER: u32 = 0;
const ENGINE_CLASS_COPY: u32 = 1;
const ENGINE_CLASS_VIDEO: u32 = 2;
const ENGINE_CLASS_VIDEO_ENHANCE: u32 = 3;
const ENGINE_CLASS_COMPUTE: u32 = 4;

/// i915 PMU sample id for cumulative engine busy time.
const I915_SAMPLE_BUSY: u64 = 0;

/// `/sys/bus/event_source/devices/i915/type` — the PMU's `perf_event_open`
/// type id. Read once at discovery; absent file ⇒ no i915 PMU on this host.
const I915_PMU_TYPE_PATH: &str = "/sys/bus/event_source/devices/i915/type";
const I915_PMU_CPUMASK_PATH: &str = "/sys/bus/event_source/devices/i915/cpumask";

/// `XE_PMU_EVENT_ENGINE_ACTIVE_TICKS` — cumulative engine active ticks (the
/// xe "busy" counter). Authoritative value from `drivers/gpu/drm/xe/xe_pmu.c`.
const XE_PMU_EVENT_ENGINE_ACTIVE_TICKS: u64 = 0x02;
/// `XE_PMU_EVENT_ENGINE_TOTAL_TICKS` — cumulative engine total ticks (the xe
/// elapsed denominator). Authoritative value from `xe_pmu.c`.
const XE_PMU_EVENT_ENGINE_TOTAL_TICKS: u64 = 0x03;

/// `/sys/bus/event_source/devices/` — scanned for the per-device `xe_<BDF>` PMU.
const EVENT_SOURCE_DEVICES: &str = "/sys/bus/event_source/devices";

/// Default xe config bitfield shifts, straight from `xe_pmu.c`. Used only when a
/// `<pmu>/format/<name>` file is absent; parsed shifts otherwise win.
const XE_DEFAULT_EVENT_SHIFT: u32 = 0;
const XE_DEFAULT_INSTANCE_SHIFT: u32 = 12;
const XE_DEFAULT_CLASS_SHIFT: u32 = 20;
const XE_DEFAULT_FUNCTION_SHIFT: u32 = 44;
const XE_DEFAULT_GT_SHIFT: u32 = 60;

/// One engine discovered for the i915 PMU, with its busy config precomputed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntelPmuEngine {
    pub(crate) label: String,
    pub(crate) class: u32,
    pub(crate) instance: u32,
    pub(crate) config: u64,
}

/// The discovered i915 PMU layout: its `type` id plus one busy config per
/// engine visible in the GT `engines/` sysfs tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntelPmuLayout {
    pub(crate) pmu_type: u32,
    /// A CPU from the PMU `cpumask` — perf_event_open rejects pid==-1 && cpu==-1
    /// with EINVAL, so the uncore counter must be pinned to this CPU.
    pub(crate) cpu: i32,
    pub(crate) engines: Vec<IntelPmuEngine>,
}

/// Discover i915 while preserving a sysfs permission/parse receipt. The
pub(crate) fn discover_intel_pmu_layout_with_receipt(
    device_path: &Path,
) -> GpuFieldRead<Option<IntelPmuLayout>> {
    let pmu_type = read_optional_u32(Path::new(I915_PMU_TYPE_PATH));
    let Some(pmu_type) = pmu_type.value.flatten() else {
        return optional_layout(pmu_type.failure);
    };

    let cpu = read_pmu_cpumask(Path::new(I915_PMU_CPUMASK_PATH));
    let engines = discover_engine_configs_with_receipt(device_path);
    let failure = preferred_gpu_failure(cpu.failure, engines.failure);
    let Some(engines) = engines.value else {
        return optional_layout(failure);
    };
    if engines.is_empty() {
        return optional_layout(failure);
    }
    GpuFieldRead {
        value: Some(Some(IntelPmuLayout {
            pmu_type,
            cpu: cpu.value.unwrap_or(0),
            engines,
        })),
        failure,
    }
}

fn optional_layout<T>(failure: Option<FailureKind>) -> GpuFieldRead<Option<T>> {
    GpuFieldRead {
        value: Some(None),
        failure,
    }
}

fn read_optional_u32(path: &Path) -> GpuFieldRead<Option<u32>> {
    match fs::read_to_string(path) {
        Ok(raw) => match raw.trim().parse::<u32>() {
            Ok(value) => GpuFieldRead::available(Some(value)),
            Err(_) => GpuFieldRead::unavailable(FailureKind::ProviderFault),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => GpuFieldRead::available(None),
        Err(error) => GpuFieldRead::unavailable(gpu_io_failure(&error, FailureKind::Unsupported)),
    }
}

/// Read the first CPU of an uncore PMU `cpumask` (e.g. `0`, `0-3`). A missing
/// optional cpumask still has the historical CPU-0 fallback, but permission or
/// malformed data remains a typed receipt.
fn read_pmu_cpumask(path: &Path) -> GpuFieldRead<i32> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return GpuFieldRead::available(0);
        }
        Err(error) => {
            return GpuFieldRead::unavailable(gpu_io_failure(&error, FailureKind::Unsupported));
        }
    };
    let digits: String = raw
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<i32>().map_or_else(
        |_| GpuFieldRead::unavailable(FailureKind::ProviderFault),
        GpuFieldRead::available,
    )
}

// ===========================================================================
// xe PMU discovery (phase B2 — Intel Core Ultra / Xe-LPG).
// ===========================================================================

/// Parsed xe config bitfield layout: the low bit of each field, read from the
/// PMU `<pmu>/format/<name>` files (kernel defaults when a file is absent).
///
/// `pack_engine_busy` reproduces the `xe_pmu.c` config encoding using ONLY the
/// parsed shifts, so a future kernel bitfield drift is caught here rather than
/// silently mis-addressing an engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct XeConfigLayout {
    event_shift: u32,
    instance_shift: u32,
    class_shift: u32,
    _function_shift: u32,
    _gt_shift: u32,
}

impl XeConfigLayout {
    /// Build the `xe_pmu.c` engine-busy config for `(event, class, instance)`.
    ///
    /// `function = 0` and `gt = 0` for engine-busy on the root GT; only event,
    /// class and instance encode. The masks are the per-field widths from
    /// `xe_pmu.c` (event 12b, instance/class 8b), so a parsed shift cannot
    /// smear a value into a neighbouring field.
    pub(crate) fn pack_engine_busy(&self, event: u64, class: u32, instance: u32) -> u64 {
        (event & 0xFFF) << self.event_shift
            | (u64::from(instance) & 0xFF) << self.instance_shift
            | (u64::from(class) & 0xFF) << self.class_shift
    }
}

/// One xe engine discovered for the xe PMU, carrying BOTH the active-ticks and
/// total-ticks perf configs (event ids `0x2`/`0x3` packed with class/instance).
/// xe returns cumulative TICKS, so a single config is insufficient — the
/// tracker needs the active/total pair to form a ratio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XePmuEngine {
    pub(crate) label: String,
    pub(crate) class: u32,
    pub(crate) instance: u32,
    pub(crate) active_config: u64,
    pub(crate) total_config: u64,
}

/// The discovered xe PMU layout: its `type` id plus one active+total config
/// pair per engine visible in the GT `engines/` sysfs tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XePmuLayout {
    pub(crate) pmu_type: u32,
    /// A CPU from the PMU `cpumask` — perf_event_open rejects pid==-1 && cpu==-1
    /// with EINVAL, so the uncore counter must be pinned to this CPU.
    pub(crate) cpu: i32,
    pub(crate) engines: Vec<XePmuEngine>,
}

/// Discover the xe PMU for one DRM device and build active+total configs per
/// engine. The receipt-bearing path is the provider-facing API; no PMU,
/// permission, or malformed sysfs state is collapsed into a number.
pub(crate) fn discover_xe_pmu_layout_with_receipt(
    device_path: &Path,
) -> GpuFieldRead<Option<XePmuLayout>> {
    let pmu_dir = resolve_xe_pmu_dir_with_receipt(device_path);
    let Some(pmu_dir) = pmu_dir.value.flatten() else {
        return optional_layout(pmu_dir.failure);
    };
    let pmu_type = read_optional_u32(&pmu_dir.join("type"));
    let Some(pmu_type) = pmu_type.value.flatten() else {
        return optional_layout(pmu_type.failure);
    };
    let cpu = read_pmu_cpumask(&pmu_dir.join("cpumask"));
    let layout = parse_xe_config_layout(&pmu_dir);
    let engines = discover_xe_engine_configs_with_receipt(device_path, &layout);
    let failure = preferred_gpu_failure(cpu.failure, engines.failure);
    let Some(engines) = engines.value else {
        return optional_layout(failure);
    };
    if engines.is_empty() {
        return optional_layout(failure);
    }
    GpuFieldRead {
        value: Some(Some(XePmuLayout {
            pmu_type,
            cpu: cpu.value.unwrap_or(0),
            engines,
        })),
        failure,
    }
}

/// Compatibility projection for the existing xe provider call site. The
/// provider-facing i915 path consumes the receipt directly; this wrapper keeps
/// the current xe fallback buildable until its state machine is migrated in a
/// later scoped change.
pub(crate) fn discover_xe_pmu_layout(device_path: &Path) -> Option<XePmuLayout> {
    discover_xe_pmu_layout_with_receipt(device_path)
        .value
        .flatten()
}

/// Resolve the `xe_<BDF>` PMU directory for one DRM device.
///
/// Prefers the PCI-slot-matched device (`xe_0000_00_02.0` for slot
/// `0000:00:02.0`); falls back to the lone `xe*` device on single-Intel-GPU
/// hosts; returns `None` when several unmatched `xe_*` devices exist (the
/// honest answer rather than a wrong-card guess).
fn resolve_xe_pmu_dir_with_receipt(device_path: &Path) -> GpuFieldRead<Option<PathBuf>> {
    let root = Path::new(EVENT_SOURCE_DEVICES);
    if let Some(slot) = read_pci_slot_for_xe(device_path) {
        let expected = format!("xe_{}", slot.replace(':', "_"));
        let candidate = root.join(&expected);
        match fs::metadata(&candidate) {
            Ok(metadata) if metadata.is_dir() => return GpuFieldRead::available(Some(candidate)),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return GpuFieldRead::unavailable(gpu_io_failure(&error, FailureKind::Unsupported));
            }
        }
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return GpuFieldRead::available(None);
        }
        Err(error) => {
            return GpuFieldRead::unavailable(gpu_io_failure(&error, FailureKind::Unsupported));
        }
    };
    let mut xe_dirs = Vec::new();
    let mut failure = None;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failure = preferred_gpu_failure(
                    failure,
                    Some(gpu_io_failure(&error, FailureKind::TemporarilyUnavailable)),
                );
                continue;
            }
        };
        let path = entry.path();
        let is_xe = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("xe"));
        if is_xe && path.is_dir() {
            xe_dirs.push(path);
        }
    }
    let value = (xe_dirs.len() == 1).then(|| xe_dirs.remove(0));
    GpuFieldRead {
        value: Some(value),
        failure,
    }
}

/// Read the `PCI_SLOT_NAME` field of `<device_path>/uevent` for the BDF match.
/// Mirrors the identity reader in the parent module (kept local so this module
/// does not reach into a private helper of a sibling for a 6-line sysfs read).
fn read_pci_slot_for_xe(device_path: &Path) -> Option<String> {
    let uevent = fs::read_to_string(device_path.join("uevent")).ok()?;
    uevent.lines().find_map(|line| {
        line.strip_prefix("PCI_SLOT_NAME=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

/// Parse the five xe config bitfields from `<pmu>/format/*`, with kernel
/// defaults for any absent file. A field whose low bit fails to parse (or is
/// `>= 64`, which would make the shift overflow a `u64`) also falls back.
fn parse_xe_config_layout(pmu_dir: &Path) -> XeConfigLayout {
    XeConfigLayout {
        event_shift: parse_format_low_bit(pmu_dir, "event").unwrap_or(XE_DEFAULT_EVENT_SHIFT),
        instance_shift: parse_format_low_bit(pmu_dir, "engine_instance")
            .unwrap_or(XE_DEFAULT_INSTANCE_SHIFT),
        class_shift: parse_format_low_bit(pmu_dir, "engine_class")
            .unwrap_or(XE_DEFAULT_CLASS_SHIFT),
        _function_shift: parse_format_low_bit(pmu_dir, "function")
            .unwrap_or(XE_DEFAULT_FUNCTION_SHIFT),
        _gt_shift: parse_format_low_bit(pmu_dir, "gt").unwrap_or(XE_DEFAULT_GT_SHIFT),
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
    let lo = first_range.split('-').next()?.trim();
    let bit: u32 = lo.parse().ok()?;
    (bit < 64).then_some(bit)
}

/// Walk the xe `tile*/gt*/engines/` tree — the directory shape the `xe` driver
/// actually registers (`drivers/gpu/drm/xe/xe_hw_engine_class_sysfs.c`) — and
/// build an active+total config pair per DISTINCT engine class.
///
/// Unlike the i915 `engines/` walk, xe exposes CLASS-ONLY directory names
/// (`rcs`, `bcs`, `vcs`, `vecs`, `ccs` — no instance digit, no `busy` node),
/// spread across one or more GTs (`tile0/gt0`, `tile0/gt1`, ...). Each class
/// collapses to instance 0 (the integrated-GPU norm — the per-class xe PMU
/// counters are system-wide) and is de-duplicated across GTs, so a class that
/// appears under both `gt0` and `gt1` yields ONE config, not two identical PMU
/// opens. The parsed xe format shifts ([`parse_xe_config_layout`]) pack the
/// event id (`0x02` active, `0x03` total) with class + instance.
///
/// This deliberately does NOT reuse the i915 [`parse_engine_instance`]: that
/// parser REQUIRES a digit instance suffix (`rcs0`), so on a Core Ultra box the
/// bare-mnemonic engine dirs (`rcs`) are rejected and the breakdown comes back
/// empty. The xe-specific [`parse_xe_engine_instance`] accepts the bare
/// mnemonic instead.
fn discover_xe_engine_configs_with_receipt(
    device_path: &Path,
    layout: &XeConfigLayout,
) -> GpuFieldRead<Vec<XePmuEngine>> {
    let directories = super::discover_gt_directories(device_path);
    let mut failure = directories.failure;
    let Some(directories) = directories.value else {
        return GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported));
    };
    // class → engine: the first sighting of a class wins. A class repeated
    // across `gt0`/`gt1` is counted once because the xe PMU is per-device, not
    // per-GT, and one counter per class is the system-wide truth.
    let mut by_class: BTreeMap<u32, XePmuEngine> = BTreeMap::new();
    for gt in directories {
        let entries = match fs::read_dir(gt.join("engines")) {
            Ok(entries) => entries,
            Err(error) => {
                failure = preferred_gpu_failure(
                    failure,
                    Some(gpu_io_failure(&error, FailureKind::Unsupported)),
                );
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    failure = preferred_gpu_failure(
                        failure,
                        Some(gpu_io_failure(&error, FailureKind::TemporarilyUnavailable)),
                    );
                    continue;
                }
            };
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            if name.starts_with('.') {
                // xe tucks a `.defaults` metadata kobject under engines/.
                continue;
            }
            let Some(parsed) = parse_xe_engine_instance(name) else {
                continue;
            };
            by_class.entry(parsed.class).or_insert(XePmuEngine {
                label: parsed.label,
                class: parsed.class,
                instance: parsed.instance,
                active_config: layout.pack_engine_busy(
                    XE_PMU_EVENT_ENGINE_ACTIVE_TICKS,
                    parsed.class,
                    parsed.instance,
                ),
                total_config: layout.pack_engine_busy(
                    XE_PMU_EVENT_ENGINE_TOTAL_TICKS,
                    parsed.class,
                    parsed.instance,
                ),
            });
        }
    }
    let engines = by_class.into_values().collect::<Vec<_>>();
    if engines.is_empty() {
        GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported))
    } else if let Some(failure) = failure {
        GpuFieldRead::partial(engines, failure)
    } else {
        GpuFieldRead::available(engines)
    }
}

/// Walk the same GT `engines/` tree as [`super::read_intel_gt_engines`] but
/// parse directory NAMES into `(class, instance)` configs without reading the
/// `busy` node — so engines are discovered even on mainline i915 where the
/// `busy` sysfs node is absent (the case the PMU fallback exists to serve).
fn discover_engine_configs_with_receipt(device_path: &Path) -> GpuFieldRead<Vec<IntelPmuEngine>> {
    let gt_directories = super::discover_gt_directories(device_path);
    let mut failure = gt_directories.failure;
    let Some(directories) = gt_directories.value else {
        return GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported));
    };

    // i915's config has no GT field, so the same `(class, instance)` seen in
    // multiple GTs is one addressable PMU event. Opening it twice would make
    // the tracker feed the same engine key twice in one tick and create a
    // false zero-time failure.
    let mut by_identity: BTreeMap<(u32, u32), IntelPmuEngine> = BTreeMap::new();
    for gt in directories {
        let entries = match fs::read_dir(gt.join("engines")) {
            Ok(entries) => entries,
            Err(error) => {
                failure = preferred_gpu_failure(
                    failure,
                    Some(gpu_io_failure(&error, FailureKind::Unsupported)),
                );
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    failure = preferred_gpu_failure(
                        failure,
                        Some(gpu_io_failure(&error, FailureKind::TemporarilyUnavailable)),
                    );
                    continue;
                }
            };
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let Some(parsed) = parse_engine_instance(name) else {
                continue;
            };
            let config = (u64::from(parsed.class)) << 12
                | (u64::from(parsed.instance)) << 4
                | I915_SAMPLE_BUSY;
            by_identity
                .entry((parsed.class, parsed.instance))
                .or_insert(IntelPmuEngine {
                    label: parsed.label,
                    class: parsed.class,
                    instance: parsed.instance,
                    config,
                });
        }
    }

    let engines = by_identity.into_values().collect::<Vec<_>>();
    if engines.is_empty() {
        GpuFieldRead::unavailable(failure.unwrap_or(FailureKind::Unsupported))
    } else if let Some(failure) = failure {
        GpuFieldRead::partial(engines, failure)
    } else {
        GpuFieldRead::available(engines)
    }
}

struct ParsedEngine {
    label: String,
    class: u32,
    instance: u32,
}

/// Parse an Intel engine sysfs directory name into `(label, class, instance)`.
///
/// Tolerates both the legacy `i915` per-instance names (`rcs0`, `bcs1`, `vcs0`,
/// `vecs0`, `ccs0` — class prefix + digit instance) and the `xe` per-class
/// names (`render`, `copy`, `compute`, `video`, `video-enhance` — collapsed to
/// instance 0). The label reuses [`super::intel_engine_label`] so the PMU path
/// and the sysfs path share one display vocabulary. Unknown names yield `None`
/// (skipped, never fabricated).
fn parse_engine_instance(name: &str) -> Option<ParsedEngine> {
    let lower = name.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("vecs") {
        // vecs before vcs: an encode engine must not be swallowed by decode.
        return build_parsed(name, ENGINE_CLASS_VIDEO_ENHANCE, rest);
    }
    if let Some(rest) = lower.strip_prefix("vcs") {
        return build_parsed(name, ENGINE_CLASS_VIDEO, rest);
    }
    if let Some(rest) = lower.strip_prefix("rcs") {
        return build_parsed(name, ENGINE_CLASS_RENDER, rest);
    }
    if let Some(rest) = lower.strip_prefix("bcs") {
        return build_parsed(name, ENGINE_CLASS_COPY, rest);
    }
    if let Some(rest) = lower.strip_prefix("ccs") {
        return build_parsed(name, ENGINE_CLASS_COMPUTE, rest);
    }
    // xe per-class names — collapsed across instances.
    let class = match lower.as_str() {
        "render" => ENGINE_CLASS_RENDER,
        "copy" | "blitter" => ENGINE_CLASS_COPY,
        "compute" => ENGINE_CLASS_COMPUTE,
        "video" | "video-decode" => ENGINE_CLASS_VIDEO,
        "video-enhance" => ENGINE_CLASS_VIDEO_ENHANCE,
        _ => return None,
    };
    Some(ParsedEngine {
        label: super::intel_engine_label(name),
        class,
        instance: 0,
    })
}

fn build_parsed(original_name: &str, class: u32, instance_suffix: &str) -> Option<ParsedEngine> {
    let instance: u32 = instance_suffix.parse().ok()?;
    Some(ParsedEngine {
        label: super::intel_engine_label(original_name),
        class,
        instance,
    })
}

/// Parse a `xe` engine sysfs directory name into `(label, class, instance)`.
///
/// The `xe` driver registers engines under `tile*/gt*/engines/` with the i915
/// per-instance vocabulary MINUS the digit suffix — bare class mnemonics
/// (`rcs`, `bcs`, `vcs`, `vecs`, `ccs`) — and on some kernels the long-form
/// class name (`render`, `copy`, ...). Mainline `xe_hw_engine_class_sysfs.c`
/// exposes neither an instance digit nor a `busy` node, and the per-class xe
/// PMU counters are system-wide, so instance is always 0. Unlike the i915
/// [`parse_engine_instance`], the bare mnemonic is VALID here. Unknown names
/// yield `None` (skipped, never fabricated).
fn parse_xe_engine_instance(name: &str) -> Option<ParsedEngine> {
    let lower = name.to_ascii_lowercase();
    let class = match xe_mnemonic_class(&lower) {
        Some(class) => class,
        None => xe_long_form_class(&lower)?,
    };
    Some(ParsedEngine {
        label: super::intel_engine_label(name),
        class,
        instance: 0,
    })
}

/// Bare i915-style mnemonic with NO required instance digit — the layout the
/// `xe` driver registers on Intel Core Ultra / Xe-LPG. `vecs` is matched before
/// `vcs` so an encode engine is never swallowed by decode. An optional all-digit
/// tail (`rcs0`) is tolerated defensively; a non-digit tail (`rcsX`) is not.
fn xe_mnemonic_class(lower: &str) -> Option<u32> {
    let (prefix, class) = if lower.starts_with("vecs") {
        ("vecs", ENGINE_CLASS_VIDEO_ENHANCE)
    } else if lower.starts_with("vcs") {
        ("vcs", ENGINE_CLASS_VIDEO)
    } else if lower.starts_with("rcs") {
        ("rcs", ENGINE_CLASS_RENDER)
    } else if lower.starts_with("bcs") {
        ("bcs", ENGINE_CLASS_COPY)
    } else if lower.starts_with("ccs") {
        ("ccs", ENGINE_CLASS_COMPUTE)
    } else {
        return None;
    };
    let tail = &lower[prefix.len()..];
    let valid_tail = tail.is_empty() || tail.bytes().all(|b| b.is_ascii_digit());
    valid_tail.then_some(class)
}

/// Long-form xe class names (`render`, `copy`, ...). Exact match only — these
/// never carry an instance suffix.
fn xe_long_form_class(lower: &str) -> Option<u32> {
    Some(match lower {
        "render" => ENGINE_CLASS_RENDER,
        "copy" | "blitter" => ENGINE_CLASS_COPY,
        "compute" => ENGINE_CLASS_COMPUTE,
        "video" | "video-decode" => ENGINE_CLASS_VIDEO,
        "video-enhance" => ENGINE_CLASS_VIDEO_ENHANCE,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "../../../../../tests/headless/engine/hardware/gpu/intel/pmu.rs"]
mod tests;
