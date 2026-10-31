//! Per-process GPU engine utilization, typed and never fabricated.
//!
//! Each open DRM file descriptor exposes a cumulative `drm-engine-<name>: <ns>`
//! busy counter in `/proc/<pid>/fdinfo/<fd>` (kernel DRM usage stats). A
//! *cumulative* nanosecond counter is only meaningful as a delta over a measured
//! interval, so the per-engine [`ProcessGpuEngineUsage::usage_pct`] is a typed
//! [`ScalarObservation`]: the first sighting seeds the baseline (a typed gap,
//! never a fabricated zero), and only a later tick produces a current 0–100%
//! single-core-equivalent rate.
//!
//! Honesty contract (the project red line): a process with no DRM render/card
//! descriptors, a vanished pid, or a permission-denied `/proc/<pid>/fd` is a
//! typed [`DeviceState`] with an empty `engines` list — never an invented
//! engine, never a fabricated zero percentage.
//!
//! Semantic engine classification:
//! Different hardware vendors and kernel drivers expose differing engine names in
//! DRM fdinfo (e.g. AMD reports `gfx`, `compute`, `sdma`, `dec`, `enc`, while Intel
//! reports `render`/`rcs`, `compute`/`ccs`, `copy`/`bcs`, `video`/`vcs`, `video-enhance`/`vecs`).
//! [`ProcessGpuEngineClass`] maps these vendor-specific names to standardized
//! engine classes ([`ProcessGpuEngineClass::Render3D`], [`ProcessGpuEngineClass::Compute`],
//! [`ProcessGpuEngineClass::Copy`], [`ProcessGpuEngineClass::VideoDecode`],
//! [`ProcessGpuEngineClass::VideoEncode`]) for cross-vendor parity.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;
use crate::core::metrics::{GpuEngineKind, ScalarObservation};

/// Semantic GPU engine class providing cross-vendor AMD, Intel, and generic DRM parity.
///
/// Unifies vendor-specific engine names reported across AMD (`amdgpu`), Intel
/// (`i915`, `xe`), NVIDIA, and standard DRM fdinfo into stable semantic classes:
///   * [`ProcessGpuEngineClass::Render3D`] — 3D graphics rendering (AMD `gfx`, Intel `render`/`rcs`, `3d`);
///   * [`ProcessGpuEngineClass::Compute`] — GPGPU compute dispatch (AMD `compute`, Intel `compute`/`ccs`);
///   * [`ProcessGpuEngineClass::Copy`] — DMA/blitter memory transfer (AMD `sdma`, Intel `copy`/`bcs`, `blitter`);
///   * [`ProcessGpuEngineClass::VideoDecode`] — Hardware video decoding (AMD `dec`/`vcn_dec`/`uvd`, Intel `video`/`vcs`);
///   * [`ProcessGpuEngineClass::VideoEncode`] — Hardware video encoding (AMD `enc`/`vcn_enc`/`uvd_enc`, Intel `video-enhance`/`vecs`).
///
/// Honesty contract: unmapped, vendor-opaque, or future engine names remain
/// [`ProcessGpuEngineClass::Unknown`] (or [`ProcessGpuEngineClass::Other`])
/// rather than guessing semantics.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ProcessGpuEngineClass {
    /// 3D graphics rendering engine (e.g. AMD `gfx`, Intel `render`/`rcs`, `3D`).
    Render3D,
    /// Compute / GPGPU engine (e.g. AMD `compute`, Intel `compute`/`ccs`).
    Compute,
    /// Memory copy / DMA / blitter engine (e.g. AMD `sdma`, Intel `copy`/`bcs`).
    Copy,
    /// Video decoding engine (e.g. AMD `dec`/`vcn_dec`/`uvd`, Intel `video`/`vcs`).
    VideoDecode,
    /// Video encoding / processing engine (e.g. AMD `enc`/`vcn_enc`/`vecs`).
    VideoEncode,
    /// Unclassified or unmapped engine class.
    Other,
    /// Unknown or unmapped engine class (default).
    #[default]
    Unknown,
}

/// Alias for [`ProcessGpuEngineClass`] for caller convenience and forward compatibility.
pub type GpuEngineClass = ProcessGpuEngineClass;

/// Alias for [`ProcessGpuEngineClass`] mirroring the `*Kind` naming convention.
pub type ProcessGpuEngineKind = ProcessGpuEngineClass;

/// Alias for [`ProcessGpuEngineClass`].
pub type ProcessGpuEngineType = ProcessGpuEngineClass;

impl ProcessGpuEngineClass {
    /// Complete list of all engine class variants.
    pub const ALL: &'static [Self] = &[
        Self::Render3D,
        Self::Compute,
        Self::Copy,
        Self::VideoDecode,
        Self::VideoEncode,
        Self::Other,
        Self::Unknown,
    ];

    /// Complete list of primary semantic classes (excluding Unknown/Other).
    pub const PRIMARY: &'static [Self] = &[
        Self::Render3D,
        Self::Compute,
        Self::Copy,
        Self::VideoDecode,
        Self::VideoEncode,
    ];

    /// Map raw engine names from DRM fdinfo (`drm-engine-<name>`), sysfs,
    /// PDH, or display strings into standardized engine classes.
    ///
    /// Cross-vendor parity:
    ///   * AMD:
    ///     - `gfx` / `gfx_0` / `gfx0` -> `Render3D`
    ///     - `compute` / `compute0` -> `Compute`
    ///     - `sdma` / `sdma0` / `sdma1` / `dma` -> `Copy`
    ///     - `dec` / `vcn_dec` / `uvd` / `jpeg_dec` -> `VideoDecode`
    ///     - `enc` / `vcn_enc` / `uvd_enc` / `vce` / `jpeg_enc` -> `VideoEncode`
    ///   * Intel:
    ///     - `render` / `rcs` / `rcs0` -> `Render3D`
    ///     - `compute` / `ccs` / `ccs0` -> `Compute`
    ///     - `copy` / `bcs` / `bcs0` / `blitter` -> `Copy`
    ///     - `video` / `vcs` / `vcs0` / `video-decode` -> `VideoDecode`
    ///     - `video-enhance` / `vecs` / `vecs0` / `video-encode` -> `VideoEncode`
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        let clean = name.trim().to_ascii_lowercase();
        if clean.is_empty() {
            return Self::Unknown;
        }

        // Strip common prefixes such as "drm-engine-" or "drm-total-busy-"
        let s = clean
            .strip_prefix("drm-engine-")
            .or_else(|| clean.strip_prefix("drm-total-busy-"))
            .or_else(|| clean.strip_prefix("drm-total-cycles-"))
            .or_else(|| clean.strip_prefix("drm-cycles-"))
            .unwrap_or(&clean);

        // 1. Video Encode (checked before Video Decode to prevent prefix collision)
        // Intel: vecs, vecs0, video-enhance, video_enhance, video-encode, video_encode
        // AMD: enc, enc0, vcn_enc, vcn-enc, uvd_enc, uvd-enc, jpeg_enc, jpeg-enc, vce, vce0
        // NVIDIA: nvenc, video encode, video processing
        if s.starts_with("vecs")
            || s.starts_with("video-enhance")
            || s.starts_with("video_enhance")
            || s.starts_with("video-encode")
            || s.starts_with("video_encode")
            || s.starts_with("videoencode")
            || s == "video encode"
            || s == "video processing"
            || s.starts_with("vcn_enc")
            || s.starts_with("vcn-enc")
            || s.starts_with("uvd_enc")
            || s.starts_with("uvd-enc")
            || s.starts_with("jpeg_enc")
            || s.starts_with("jpeg-enc")
            || s.starts_with("vce")
            || s.starts_with("nvenc")
            || s == "enc"
            || s.starts_with("enc") && s[3..].bytes().all(|b| b.is_ascii_digit())
            || s == "encode"
            || s == "encoder"
        {
            return Self::VideoEncode;
        }

        // 2. Video Decode
        // Intel: vcs, vcs0, video, video-decode, video_decode
        // AMD: dec, dec0, vcn_dec, vcn-dec, uvd, uvd_dec, uvd-dec, jpeg_dec, jpeg-dec, vcn, jpeg
        // NVIDIA: nvdec, video decode
        if s.starts_with("vcs")
            || s == "video"
            || s.starts_with("video-decode")
            || s.starts_with("video_decode")
            || s.starts_with("videodecode")
            || s == "video decode"
            || s.starts_with("vcn_dec")
            || s.starts_with("vcn-dec")
            || s.starts_with("uvd")
            || s.starts_with("jpeg_dec")
            || s.starts_with("jpeg-dec")
            || s == "jpeg"
            || s == "vcn"
            || s.starts_with("nvdec")
            || s == "dec"
            || s.starts_with("dec") && s[3..].bytes().all(|b| b.is_ascii_digit())
            || s == "decode"
            || s == "decoder"
        {
            return Self::VideoDecode;
        }

        // 3. Copy / DMA / Blitter
        // Intel: bcs, bcs0, copy, blitter
        // AMD: sdma, sdma0, sdma1, dma
        // Generic: memory (copy), transfer, blt
        if s.starts_with("bcs")
            || s.starts_with("sdma")
            || s.starts_with("dma")
            || s == "copy"
            || s == "blitter"
            || s == "blt"
            || s == "memory (copy)"
            || s == "transfer"
        {
            return Self::Copy;
        }

        // 4. Compute / GPGPU
        // Intel: ccs, ccs0, compute
        // AMD: compute, compute0, comp
        if s.starts_with("ccs") || s.starts_with("compute") || s == "comp" {
            return Self::Compute;
        }

        // 5. Render / 3D Graphics
        // Intel: rcs, rcs0, render, 3d, render/3d, graphics (3d), graphics
        // AMD: gfx, gfx0, gfx_0, 3d
        // Generic: render3d, render_3d, render-3d
        if s.starts_with("rcs")
            || s.starts_with("gfx")
            || s.starts_with("render")
            || s == "3d"
            || s == "render/3d"
            || s == "graphics (3d)"
            || s == "graphics"
            || s == "render3d"
            || s == "render_3d"
            || s == "render-3d"
        {
            return Self::Render3D;
        }

        Self::Unknown
    }

    /// Alias for [`Self::from_name`].
    #[must_use]
    pub fn from_engine_name(name: &str) -> Self {
        Self::from_name(name)
    }

    /// Alias for [`Self::from_name`].
    #[must_use]
    pub fn from_display_name(name: &str) -> Self {
        Self::from_name(name)
    }

    /// Canonical display label (e.g. `"Render/3D"`, `"Compute"`, `"Copy"`,
    /// `"Video Decode"`, `"Video Encode"`).
    #[must_use]
    pub const fn display_label(&self) -> &'static str {
        match self {
            Self::Render3D => "Render/3D",
            Self::Compute => "Compute",
            Self::Copy => "Copy",
            Self::VideoDecode => "Video Decode",
            Self::VideoEncode => "Video Encode",
            Self::Other => "Other",
            Self::Unknown => "Unknown",
        }
    }

    /// Alias for [`Self::display_label`].
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        self.display_label()
    }

    /// Canonical snake_case string identifier.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Render3D => "render_3d",
            Self::Compute => "compute",
            Self::Copy => "copy",
            Self::VideoDecode => "video_decode",
            Self::VideoEncode => "video_encode",
            Self::Other => "other",
            Self::Unknown => "unknown",
        }
    }

    /// True if this class is 3D graphics rendering.
    #[must_use]
    pub const fn is_render_3d(&self) -> bool {
        matches!(self, Self::Render3D)
    }

    /// True if this class is 3D graphics rendering (alias for [`Self::is_render_3d`]).
    #[must_use]
    pub const fn is_3d(&self) -> bool {
        matches!(self, Self::Render3D)
    }

    /// True if this class is compute / GPGPU.
    #[must_use]
    pub const fn is_compute(&self) -> bool {
        matches!(self, Self::Compute)
    }

    /// True if this class is copy / DMA / blitter.
    #[must_use]
    pub const fn is_copy(&self) -> bool {
        matches!(self, Self::Copy)
    }

    /// True if this class is video decoding.
    #[must_use]
    pub const fn is_video_decode(&self) -> bool {
        matches!(self, Self::VideoDecode)
    }

    /// True if this class is video encoding.
    #[must_use]
    pub const fn is_video_encode(&self) -> bool {
        matches!(self, Self::VideoEncode)
    }

    /// True if this class is video (either decode or encode).
    #[must_use]
    pub const fn is_video(&self) -> bool {
        matches!(self, Self::VideoDecode | Self::VideoEncode)
    }

    /// True if this class is a proven semantic class (not Unknown or Other).
    #[must_use]
    pub const fn is_classified(&self) -> bool {
        !matches!(self, Self::Unknown | Self::Other)
    }
}

impl From<ProcessGpuEngineClass> for GpuEngineKind {
    fn from(class: ProcessGpuEngineClass) -> Self {
        match class {
            ProcessGpuEngineClass::Render3D => Self::Render,
            ProcessGpuEngineClass::Compute => Self::Compute,
            ProcessGpuEngineClass::Copy => Self::Copy,
            ProcessGpuEngineClass::VideoDecode => Self::VideoDecode,
            ProcessGpuEngineClass::VideoEncode => Self::VideoEncode,
            ProcessGpuEngineClass::Other | ProcessGpuEngineClass::Unknown => Self::Unknown,
        }
    }
}

impl From<GpuEngineKind> for ProcessGpuEngineClass {
    fn from(kind: GpuEngineKind) -> Self {
        match kind {
            GpuEngineKind::Render => Self::Render3D,
            GpuEngineKind::Compute => Self::Compute,
            GpuEngineKind::Copy => Self::Copy,
            GpuEngineKind::VideoDecode => Self::VideoDecode,
            GpuEngineKind::VideoEncode => Self::VideoEncode,
            GpuEngineKind::Unknown => Self::Unknown,
        }
    }
}

/// One GPU engine's utilization for a single process.
///
/// `name` is the engine class parsed verbatim from the fdinfo key (for example
/// `render`, `video`, `copy`, or a vendor-specific identifier like `rcs` or
/// `gfx`) so the caller never loses information. `usage_pct` is
/// single-core-equivalent — the same convention the container CPU rollup and
/// the system-wide Intel engine tracker use — and is `Unavailable` on the first
/// sample (no delta yet) or when the cumulative counter rolled back (driver
/// reset / fd recycled).
///
/// `engine_time_ns` is the cumulative busy time observed this tick, aggregated
/// across every DRM descriptor the process holds. Unlike the rate, the
/// cumulative counter is observable on every healthy read, so it stays
/// `Available` from the first sighting — giving an honest cold-start reading
/// even before a rate can be computed.
///
/// `engine_cycles` carries the xe-driver cycle counter when the kernel exposes
/// cycles instead of busy nanoseconds (`drm-total-cycles-<class>` /
/// `drm-cycles-<class>`, kernel DRM usage stats). Cycles alone cannot be
/// converted to a utilization percentage without the GT clock, so on a
/// cycles-only source `usage_pct` stays a typed gap and the raw cycle count is
/// the honest observable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessGpuEngineUsage {
    /// Engine class as it appeared in the fdinfo key.
    pub name: String,
    /// Single-core-equivalent 0–100% utilization, typed for the cold-start gap
    /// and counter rollback. A cycles-only source (xe) keeps this a typed gap.
    pub usage_pct: ScalarObservation<f32>,
    /// Cumulative DRM busy time for this engine across the process's DRM
    /// descriptors, in nanoseconds. `None`-typed when the driver exposes
    /// cycles instead of busy time (xe).
    pub engine_time_ns: ScalarObservation<u64>,
    /// Cumulative engine cycle counter (xe `drm-total-cycles-<class>` /
    /// `drm-cycles-<class>`). `Unknown` when the driver reports busy ns (i915).
    #[serde(default)]
    pub engine_cycles: ScalarObservation<u64>,
}

impl ProcessGpuEngineUsage {
    /// Construct a new process GPU engine usage observation.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        usage_pct: ScalarObservation<f32>,
        engine_time_ns: ScalarObservation<u64>,
        engine_cycles: ScalarObservation<u64>,
    ) -> Self {
        Self {
            name: name.into(),
            usage_pct,
            engine_time_ns,
            engine_cycles,
        }
    }

    /// Resolve the semantic engine class from this engine's name.
    ///
    /// Cross-vendor parity: maps AMD (`gfx`, `compute`, `sdma`, `dec`, `enc`)
    /// and Intel (`render`/`rcs`, `compute`/`ccs`, `copy`/`bcs`, `video`/`vcs`,
    /// `video-enhance`/`vecs`) to standard engine classes.
    #[must_use]
    pub fn engine_class(&self) -> ProcessGpuEngineClass {
        ProcessGpuEngineClass::from_name(&self.name)
    }

    /// Alias for [`Self::engine_class`].
    #[must_use]
    pub fn class(&self) -> ProcessGpuEngineClass {
        self.engine_class()
    }

    /// Alias for [`Self::engine_class`].
    #[must_use]
    pub fn kind(&self) -> ProcessGpuEngineClass {
        self.engine_class()
    }
}

/// The per-process GPU engine breakdown plus a typed collection state.
///
/// `state` describes the `/proc/<pid>/fd` + `fdinfo` collection as a whole: a
/// permission-denied descriptor directory is `PermissionDenied`, a vanished pid
/// is `Stale`, and a healthy process with no DRM descriptors is `Healthy` with
/// an empty `engines` list (an honest empty, not an unknown).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProcessGpuEngines {
    /// Typed health of the fd/fdinfo collection that produced this breakdown.
    pub state: DeviceState,
    /// Per-engine entries, ordered by ascending engine name for stable diffing.
    pub engines: Vec<ProcessGpuEngineUsage>,
}

impl ProcessGpuEngines {
    /// A healthy breakdown over no engines — the honest representation of a
    /// live, non-GPU process (no `/dev/dri/` descriptors open).
    #[must_use]
    pub fn empty_healthy(now_ms: u64) -> Self {
        Self {
            state: DeviceState::healthy(now_ms),
            engines: Vec::new(),
        }
    }

    /// A breakdown whose source was typed-unavailable (EACCES on
    /// `/proc/<pid>/fd`, a vanished pid, ...). The engine list is always empty
    /// here: a failed source must never retain fabricated rows.
    #[must_use]
    pub fn unavailable(state: DeviceState) -> Self {
        Self {
            state,
            engines: Vec::new(),
        }
    }

    /// Find the first engine matching a specific semantic class.
    #[must_use]
    pub fn find_by_class(&self, class: ProcessGpuEngineClass) -> Option<&ProcessGpuEngineUsage> {
        self.engines.iter().find(|e| e.engine_class() == class)
    }

    /// Return an iterator over engines matching a specific semantic class.
    pub fn filter_by_class(
        &self,
        class: ProcessGpuEngineClass,
    ) -> impl Iterator<Item = &ProcessGpuEngineUsage> {
        self.engines
            .iter()
            .filter(move |e| e.engine_class() == class)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_telemetry_gpu_engines_tests.rs"]
mod tests;
