//! Live renderer facts behind the optical-glass capability.
//!
//! The neutral theme owns the fallback decision; this module owns only the
//! Bevy renderer fact. A capability ceiling is reported only when the render
//! adapter resource has been published by `RenderPlugin`. No adapter means no
//! live GPU pipeline, so the renderer honestly reports `UNSUPPORTED` and the
//! theme returns Painted. The ceiling is the set of operations the production
//! WESL and material bridge implement; it is *proved* by the ignored GPU test
//! and recorded by the `bevy-optical-gpu` standard gate, never by this constant.

use bevy::render::render_resource::WgpuAdapterInfo;
use bevy::render::renderer::RenderAdapterInfo;
use taskmanager_theme::optical_glass::{OpticalGlassCapability, OpticalGlassOperations};

/// The operations the production WESL plus the integrated backdrop pipeline
/// implement. `scripts/quality/bevy-optical-gpu-gate.sh` proves them.
pub(crate) const OPTICAL_GLASS_PROVEN_OPERATIONS: OpticalGlassOperations =
    OpticalGlassOperations::COMPLETE;

/// Glass layers the production composition resolves at once.
///
/// The first layer samples the painted lower composite. The second samples
/// the chain target, which republishes that composite and re-renders the
/// first layer's real optical material over it. A third layer has no chain
/// target of its own, so the shared theme still fails it closed.
pub(crate) const OPTICAL_GLASS_MAX_NESTED_LAYERS: u8 = 2;

/// The largest physical blur radius the production sampler accepts. Larger
/// radii fall back through the theme decision instead of silently sampling.
pub(crate) const OPTICAL_GLASS_MAX_BLUR_PX: u32 = 64;

/// Report the renderer capability for one live composition.
///
/// `adapter` is Bevy's published adapter identity. `None` is exactly the
/// headless/no-adapter case the theme must fail closed on.
#[must_use]
pub(crate) fn optical_glass_capability(
    adapter: Option<&RenderAdapterInfo>,
) -> OpticalGlassCapability {
    if adapter.is_none_or(adapter_is_software) {
        return OpticalGlassCapability::UNSUPPORTED;
    }
    OpticalGlassCapability::reported_by_renderer(
        OPTICAL_GLASS_PROVEN_OPERATIONS,
        OPTICAL_GLASS_MAX_NESTED_LAYERS,
        OPTICAL_GLASS_MAX_BLUR_PX,
    )
}

/// True when wgpu classifies the published adapter as a CPU/software device.
///
/// A software adapter still compiles and runs the WESL, so the proof test can
/// pass on it, but the receipt must label the adapter honestly. Bevy does not
/// re-export `wgpu::DeviceType`, so the discriminant comes from its `Debug`
/// rendering rather than a second direct wgpu dependency.
#[must_use]
pub(crate) fn adapter_is_software(adapter: &RenderAdapterInfo) -> bool {
    let info: &WgpuAdapterInfo = adapter;
    format!("{:?}", info.device_type) == "Cpu"
}

#[cfg(test)]
#[path = "../../../tests/headless/optical_capability.rs"]
mod tests;
