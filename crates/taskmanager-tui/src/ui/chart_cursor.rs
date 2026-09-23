//! Keyboard chart cursor for the TUI's history charts (the terminal port of a
//! pointer hover).
//!
//! The terminal has no hover surface, so the TUI exposes the Iced chart-hover
//! semantics (`mc05-graph-hover` / `mc05-hover-index`) as a keyboard cursor:
//! `←`/`→` on the Performance·CPU chart move a selected sample index across the
//! rendered history series, and the chart paints the same per-sample readout a
//! pointer hover would produce. The cursor is honest absence — no index and no
//! readout — for a window with fewer than two samples (nothing to hover), and
//! it never fabricates a value for a gap sample (the shared dash). The index is
//! clamped into the live window on every move and re-checked at paint time, so
//! a shrinking history can never point past the series.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_shell::InputDispatch;
use taskmanager_shell::presentation::{missing_value, trend};

use crate::{PerfDevice, TuiApp};

/// Minimum samples a series needs before a hover/cursor is meaningful. A
/// single sample cannot be stepped across, so the cursor stays absent rather
/// than pointing at a fabricated neighbour.
pub(super) const MIN_CURSOR_SAMPLES: usize = 2;

/// The cursor index after moving `delta` samples, or `None` when the window is
/// too short to hover. `None` input opens the cursor on the newest sample (the
/// right edge, where a pointer hover would land first) and then applies
/// `delta`; an existing cursor is clamped into `0..len`. Never panics and never
/// returns an out-of-range index.
#[must_use]
pub(crate) fn move_cursor(current: Option<usize>, delta: isize, len: usize) -> Option<usize> {
    if len < MIN_CURSOR_SAMPLES {
        return None;
    }
    let last = len - 1;
    let base = current.map_or(last, |index| index.min(last));
    Some(base.saturating_add_signed(delta).min(last))
}

/// The per-sample readout line a pointer hover would paint: the sample's
/// position in the window and its formatted value. A non-finite sample keeps
/// the shared missing-value dash instead of a fabricated number. `None` when
/// the index is outside the window (no cursor to read).
#[must_use]
pub(crate) fn readout_line(
    label: &str,
    samples: &[f32],
    index: usize,
    format_value: impl Fn(f32) -> String,
) -> Option<String> {
    let value = samples.get(index)?;
    let shown = if value.is_finite() {
        format_value(*value)
    } else {
        missing_value()
    };
    Some(format!(
        "{label} · {}/{} · {shown}",
        index + 1,
        samples.len()
    ))
}

impl TuiApp {
    /// Move the Performance·CPU chart cursor by `delta` samples. The cursor is
    /// clamped against the live CPU utilization window; a window too short to
    /// hover clears it. Local state only — no platform effect.
    pub(crate) fn move_chart_cursor(&mut self, delta: isize) {
        let len = trend::cpu_usage_percent(&self.history).len();
        self.chart_cursor = move_cursor(self.chart_cursor, delta, len);
    }
}

/// The Performance·CPU chart cursor key system: `←`/`→` step the sample cursor
/// and consume the key. Scoped to the bare chords on the CPU device so a
/// modifier chord still falls through to the shared router.
pub(crate) fn chart_cursor_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if app.page() != taskmanager_application::AppPage::Performance
        || app.perf_device != PerfDevice::Cpu
        || key.modifiers.intersects(
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::SHIFT,
        )
    {
        return InputDispatch::Unhandled;
    }
    let delta = match key.code {
        KeyCode::Left => -1,
        KeyCode::Right => 1,
        _ => return InputDispatch::Unhandled,
    };
    app.move_chart_cursor(delta);
    InputDispatch::Consumed
}
