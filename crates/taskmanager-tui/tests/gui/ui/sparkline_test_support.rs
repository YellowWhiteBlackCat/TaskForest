//! Test-only sparkline helpers shared across the render test modules.

use super::SPARKLINE_MAX_SAMPLES;
use super::device_trend_with;

/// Render a per-device one-line trend from a history window: a bounded
/// block-character sparkline when at least two finite samples exist, otherwise
/// the dotted "collecting" placeholder. The window is bounded to
/// [`SPARKLINE_MAX_SAMPLES`] first so the trend keeps a stable width as the
/// 64-sample ring fills. Each device plots its OWN window (a disk/NIC/GPU
/// identifier resolved through the shared `MetricHistory` per-device API), so a
/// device with no history yet honestly renders the placeholder instead of a
/// fabricated flat line.
pub(crate) fn device_trend(samples: &[f32]) -> String {
    device_trend_with(samples, SPARKLINE_MAX_SAMPLES)
}
