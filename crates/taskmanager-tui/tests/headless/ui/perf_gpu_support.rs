//! Test-only GPU fact-line helpers: the Unicode-repertoire entry points the
//! ladder regression tests consume. Production renders pass the resolved
//! glyph mode through [`super::gpu_fact_lines_with_theme`].

use ratatui::text::Line;

use super::{GpuFactDensity, gpu_fact_lines_with_theme};
use crate::TuiTheme;
use taskmanager_core::core::metrics::GpuMetrics;

pub(crate) fn gpu_fact_lines(gpus: &[GpuMetrics], density: GpuFactDensity) -> Vec<Line<'static>> {
    gpu_fact_lines_with_theme(gpus, TuiTheme::default(), density)
}
