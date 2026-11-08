//! Single-source block-character sparkline for the TUI.
//!
//! # Component contract
//!
//! This module is the TUI's trend/sparkline component. Its `_in` pure
//! function family (`sparkline_in`, `device_trend_in`,
//! `device_dual_trend_in`, `process_cpu_trend_in`) is the trend API every
//! TUI surface renders through, and the app-history trend column is built on
//! the same published ramps ([`SPARKLINE_BLOCKS`],
//! [`SPARKLINE_ASCII_BLOCKS`]) and bounded window. ANY trend drawing in this
//! frontend must go through this module — a hand-rolled ramp, ladder, or gap
//! glyph outside it is drift — so both glyph repertoires and the
//! gap/placeholder vocabulary stay single-sourced for every trend view.
//!
//! Owned by the TUI: maps a bounded slice of finite samples onto one of two
//! single-cell intensity ramps selected by the terminal profile's glyph
//! repertoire — the Unicode half-block ramp, or an all-ASCII ink ladder at
//! paint time so an ASCII-only terminal never depends on the frame-level cell
//! rewrite to keep a trend readable. Both the App-history trend column and the
//! Performance trend strip project from this one implementation so the two
//! trend views can never drift apart. Per-row min/max normalization (Tufte
//! sparkline semantics) shows each series' recent SHAPE; the absolute value
//! lives in the adjacent text column when the caller renders one. The
//! dual-direction rows (disk read/write, NIC rx/tx) are the one deliberate
//! exception: they share a single min/max so the two directions stay
//! comparable in amplitude.

use ratatui::style::Style;
use ratatui::text::Span;
use taskmanager_application::history_decimation::lttb_gap_preserving;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::{
    bytes, graph_summary, graph_summary_text, latest_samples, missing_value,
};
use zhuge_ui_core::value::interval_fraction;

// A TUI sample that cannot be placed sits at the ramp's start; a constant
// series instead shows the mid-ramp block, which the caller declares below.
const FLAT_SAMPLE_AT_START: f32 = 0.0;
const FLAT_SERIES_SHOWS_MID_RAMP: f32 = 0.5;

use crate::TuiGlyphMode;
use taskforest_core::core::percent::percent_text;

/// How many of the most-recent samples a trend sparkline renders. Keeps every
/// trend cell a stable width regardless of the 64-sample window depth, and
/// keeps the per-frame allocation bounded.
pub(super) const SPARKLINE_MAX_SAMPLES: usize = 24;

/// The Unicode block ramp used for the sparkline, ordered low→high. Published
/// (`pub(super)`) as the component's shared ramp: the app-history trend column
/// indexes the same array, so both trend views carry the same level at the
/// same index by construction instead of by mirrored comment.
pub(super) const SPARKLINE_BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// The ASCII intensity ladder used when the terminal profile selects
/// [`TuiGlyphMode::Ascii`], ordered low→high and index-aligned with
/// [`SPARKLINE_BLOCKS`] so both repertoires carry the same level at the same
/// index. Every step is one ASCII byte, so a painted trend occupies exactly
/// one terminal cell per sample in every locale and no later cell-level
/// fallback ever needs to rewrite it. The steps form a monotonic ink gradient
/// (`' '` < `'.'` < `':'` < `'-'` < `'='` < `'+'` < `'*'` < `'#'`), so a
/// higher value always paints the visually stronger character — the level is
/// carried by the glyph itself and survives a colorless terminal. Published
/// (`pub(super)`) as the component's shared ladder: the app-history trend
/// column indexes the same array.
pub(super) const SPARKLINE_ASCII_BLOCKS: [char; 8] = [' ', '.', ':', '-', '=', '+', '*', '#'];

/// The ramp character for one clamped normalized level (index always 0..=7)
/// under the selected glyph repertoire.
const fn block_for(mode: TuiGlyphMode, index: usize) -> char {
    match mode {
        TuiGlyphMode::Unicode => SPARKLINE_BLOCKS[index],
        TuiGlyphMode::Ascii => SPARKLINE_ASCII_BLOCKS[index],
    }
}

/// The gap glyph for one missing sample under the selected repertoire. Both
/// repertoires pick a character outside their own ramp so an explicit gap can
/// never read as a real level.
const fn gap_for(mode: TuiGlyphMode) -> char {
    match mode {
        TuiGlyphMode::Unicode => DEVICE_TREND_GAP,
        TuiGlyphMode::Ascii => ASCII_TREND_GAP,
    }
}

/// The "collecting" placeholder under the selected repertoire. The ASCII form
/// keeps the Unicode form's four-cell width so trend columns do not shift
/// between profiles.
const fn placeholder_for(mode: TuiGlyphMode) -> &'static str {
    match mode {
        TuiGlyphMode::Unicode => DEVICE_TREND_PLACEHOLDER,
        TuiGlyphMode::Ascii => ASCII_TREND_PLACEHOLDER,
    }
}

/// Map a slice of samples onto a per-row-normalized sparkline in the given
/// terminal glyph repertoire. An empty slice renders empty; a constant or
/// non-finite range renders as a flat mid-ramp line — never a panic and never
/// a fabricated trend. The Unicode repertoire keeps the historical block ramp
/// byte for byte; the ASCII repertoire paints the same normalized levels
/// through [`SPARKLINE_ASCII_BLOCKS`] at paint time, so an ASCII-only terminal
/// reads a monotonic gradient straight from the renderer instead of the
/// collapsed output of a post-paint cell rewrite.
pub(super) fn sparkline_in(mode: TuiGlyphMode, samples: &[f32]) -> String {
    if samples.is_empty() {
        return String::new();
    }
    let max = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let min = samples.iter().copied().fold(f32::INFINITY, f32::min);
    let range = max - min;
    samples
        .iter()
        .map(|&value| {
            // Two different answers live here, so the range test is not redundant
            // with the projection's own: a constant or unmeasurable range renders the
            // mid-ramp block, while inside a real range a non-finite sample stays at
            // the ramp's start. One declared `flat` cannot answer both.
            let normalized = if !range.is_finite() || range <= 0.0 {
                0.5_f32
            } else {
                interval_fraction(value, min, max, FLAT_SAMPLE_AT_START)
            };
            let idx = ((normalized * 7.0).round() as usize).min(7);
            block_for(mode, idx)
        })
        .collect()
}

/// Minimum finite samples a per-device window needs before its trend renders a
/// sparkline. Below this the dotted placeholder is rendered: a single sample
/// cannot show a SHAPE, and drawing it as a one-block line would read as a
/// fabricated trend.
const MIN_DEVICE_TREND_SAMPLES: usize = 2;

/// The dotted "collecting" placeholder rendered when a per-device window has
/// fewer than [`MIN_DEVICE_TREND_SAMPLES`] finite samples — honest absence,
/// never a fabricated flat line. Uses the mid-dot (distinct from every ramp
/// block) so a reader can tell "no data yet" apart from a real flat trend.
const DEVICE_TREND_PLACEHOLDER: &str = "····";

/// Test-only sparkline helpers shared across the render test modules.
#[cfg(test)]
#[path = "../../tests/gui/ui/sparkline_test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "../../tests/gui/ui/sparkline_tests.rs"]
mod tests;

/// [`device_trend`] with an explicit window (the persisted
/// graph-data-points preference). Production callers go through
/// [`device_trend_in`]; this wrapper remains as the Unicode-regression entry
/// for the paint-time ladder tests.
/// [`device_trend_with`] for an explicit terminal glyph repertoire: the
/// cold-start placeholder and the ramp are both selected for the repertoire at
/// paint time.
pub(super) fn device_trend_in(mode: TuiGlyphMode, samples: &[f32], window: usize) -> String {
    if samples.len() < MIN_DEVICE_TREND_SAMPLES {
        return placeholder_for(mode).to_string();
    }
    sparkline_in(mode, latest_samples(samples, window))
}

/// The glyph rendered for a missing sample (`NaN`) inside an otherwise live
/// dual-direction row — the same mid-dot as the cold-start placeholder,
/// distinct from every ramp block, so an explicit per-direction gap never
/// reads as a fabricated drop to the baseline block.
const DEVICE_TREND_GAP: char = '·';

/// The ASCII gap and cold-start placeholder for [`TuiGlyphMode::Ascii`]: a
/// visible underline that sits outside [`SPARKLINE_ASCII_BLOCKS`], carrying
/// the same "explicit absence, never a baseline level" semantics as the
/// Unicode mid-dots. The placeholder repeats the gap glyph and keeps the
/// Unicode `····` four-cell width, so trend columns do not shift between
/// profiles.
const ASCII_TREND_GAP: char = '_';
const ASCII_TREND_PLACEHOLDER: &str = "____";

/// The two rows of a per-device dual-direction trend (disk read/write, NIC
/// rx/tx). `primary` is the first-listed direction (disk read, NIC receive),
/// `secondary` its companion. Both rows normalize against the ONE min/max
/// shared by the pair, so a direction that dominates reads as dominant — the
/// terminal form of the iced two-series chart's shared-scale contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DeviceDualTrend {
    pub(super) primary: String,
    pub(super) secondary: String,
}

/// Project the two split-direction windows of one device (oldest→newest,
/// `NaN` = explicit gap) onto a pair of bounded, shared-scale sparkline rows.
/// The Unicode entry point. Each direction keeps its own honest state: a row
/// with fewer than [`MIN_DEVICE_TREND_SAMPLES`] finite samples renders the
/// dotted "collecting" placeholder even while its companion plots, and a `NaN`
/// inside a live row renders the gap glyph instead of a baseline block.
/// Production callers go through [`device_dual_trend_in`]; this wrapper
/// remains as the Unicode-regression entry for the ladder tests.
/// [`device_dual_trend_with`] for an explicit terminal glyph repertoire: the
/// ramp, the gap glyph and the cold-start placeholder are all selected for the
/// repertoire at paint time.
pub(super) fn device_dual_trend_in(
    mode: TuiGlyphMode,
    primary_samples: &[f32],
    secondary_samples: &[f32],
    window: usize,
) -> DeviceDualTrend {
    let primary_window = latest_samples(primary_samples, window);
    let secondary_window = latest_samples(secondary_samples, window);
    // One shared extent over BOTH windows' finite samples, so the pair's
    // amplitudes are directly comparable row-to-row.
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for value in primary_window
        .iter()
        .chain(secondary_window.iter())
        .copied()
        .filter(|value| value.is_finite())
    {
        min = min.min(value);
        max = max.max(value);
    }
    DeviceDualTrend {
        primary: dual_row_in(mode, primary_window, min, max),
        secondary: dual_row_in(mode, secondary_window, min, max),
    }
}

/// One dual-direction row: the dotted placeholder when this direction holds
/// fewer than [`MIN_DEVICE_TREND_SAMPLES`] finite samples, otherwise the
/// shared-normalization ramp where non-finite samples render the repertoire's
/// gap glyph.
fn dual_row_in(mode: TuiGlyphMode, samples: &[f32], min: f32, max: f32) -> String {
    if samples.iter().filter(|value| value.is_finite()).count() < MIN_DEVICE_TREND_SAMPLES {
        return placeholder_for(mode).to_string();
    }
    samples
        .iter()
        .map(|&value| {
            if !value.is_finite() {
                return gap_for(mode);
            }
            // A constant pair (a zero, negative or unmeasurable range) renders as
            // the mid-ramp block, which is the answer this surface declares.
            let normalized = interval_fraction(value, min, max, FLAT_SERIES_SHOWS_MID_RAMP);
            block_for(mode, ((normalized * 7.0).round() as usize).min(7))
        })
        .collect()
}

/// One row of the two-row dual-direction trend: `label` padded to
/// `label_width` (the pair's common label width, so the two sparklines start
/// at the same column) followed by that direction's trend string. The label
/// stays unstyled; the trend carries the caller's direction color.
pub(super) fn dual_trend_line(
    label: &str,
    label_width: usize,
    trend: &str,
    style: Style,
) -> ratatui::text::Line<'static> {
    ratatui::text::Line::from(vec![
        Span::raw("  "),
        Span::raw(format!("{} ", super::text::pad_cells(label, label_width))),
        Span::styled(trend.to_owned(), style),
    ])
}

/// Per-row CPU-history trend for one process: a bounded block-character
/// sparkline when at least two finite samples exist, otherwise the dotted
/// "collecting" placeholder. Mirrors the gpui per-row sparkline
/// (`processes_view/rows/cells.rs`) and the iced `process_sparkline` —
/// per-row min/max normalization is built into the shared [`sparkline`]
/// primitive so two rows aren't meant to be compared in amplitude.
///
/// Aggregate rows (group headers) and tree PARENT nodes carry no single CPU
/// history; the renderer filters those out and emits an honest `—` instead
/// of calling this helper. This helper only owns the leaf-row trend shape
/// (real sparkline vs the cold-start placeholder), reusing the SAME
/// finite-sample gate and bounded window as the per-device trends so the
/// two trend views can never drift apart. Production callers go through
/// [`process_cpu_trend_in`]; this wrapper remains as the Unicode-regression
/// entry for the ladder tests.
/// [`process_cpu_trend`] for an explicit terminal glyph repertoire, so the
/// per-row table can paint the same finite-sample gate and bounded window
/// through the ASCII ladder on ASCII-only terminals.
pub(super) fn process_cpu_trend_in(mode: TuiGlyphMode, samples: &[f32]) -> String {
    device_trend_in(mode, samples, SPARKLINE_MAX_SAMPLES)
}

/// Units used by the compact Latest/Avg/Peak line beneath a per-device trend.
/// The reduction itself stays in the shared shell presentation layer; this
/// enum only selects the renderer-facing suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeviceSummaryUnit {
    Percent,
    BytesPerSecond,
    Rpm,
    /// Power draw in watts.
    Watts,
    /// Temperature in °C.
    Celsius,
}

/// Format one reduced statistic with its renderer-facing unit suffix. The
/// Unicode suffixes keep the historical spellings byte for byte; the ASCII
/// repertoire substitutes the degree-free Celsius form so no cell ever needs
/// the post-paint fallback (which would turn `°` into `?`).
fn summary_value(value: f32, unit: DeviceSummaryUnit, mode: TuiGlyphMode) -> String {
    match unit {
        DeviceSummaryUnit::Percent => percent_text(value),
        DeviceSummaryUnit::BytesPerSecond => {
            if value.is_finite() && value >= 0.0 {
                // `bytes` spells binary tiers in ASCII (B/KiB/MiB/GiB), so the
                // rate suffix is repertoire-safe in both modes.
                format!("{}/s", bytes(value.round() as u64))
            } else {
                missing_value()
            }
        }
        DeviceSummaryUnit::Rpm => format!("{value:.0} RPM"),
        DeviceSummaryUnit::Watts => format!("{value:.1} W"),
        DeviceSummaryUnit::Celsius => match mode {
            TuiGlyphMode::Unicode => format!("{value:.0}°C"),
            TuiGlyphMode::Ascii => format!("{value:.0}C"),
        },
    }
}

/// The field separator between the Latest/Avg/Peak statistics of a summary
/// line, per repertoire. Unicode keeps the historical mid-dot spacing; ASCII
/// uses a pipe, which cannot collide with the ASCII missing-value dash (`—`
/// falls back to `-`) the way a literal `-` separator would.
const fn summary_separator_for(mode: TuiGlyphMode) -> &'static str {
    match mode {
        TuiGlyphMode::Unicode => " · ",
        TuiGlyphMode::Ascii => " | ",
    }
}

/// [`device_summary_line`] with an explicit terminal glyph repertoire: the
/// separator and the unit suffixes are both selected for the repertoire at
/// paint time, so an ASCII-only terminal reads a plain-ASCII summary line
/// straight from the renderer instead of a `?`-riddled output of the
/// post-paint cell rewrite. Unicode output stays byte-identical to the
/// historical mid-dot form.
pub(super) fn device_summary_line_in(
    mode: TuiGlyphMode,
    label: &str,
    samples: &[f32],
    unit: DeviceSummaryUnit,
) -> Option<String> {
    let summary = graph_summary(samples)?;
    let separator = summary_separator_for(mode);
    Some(graph_summary_text(
        &summary,
        Some(label),
        separator,
        |value| summary_value(value, unit, mode),
    ))
}

/// The glyph for an explicit recording-downtime gap under the selected
/// repertoire. Both repertoires pick a character outside their own ramp so a
/// gap can never read as a real level; the ASCII underline mirrors the shared
/// sparkline's ASCII gap. (The Unicode gap stays the renderer-local space —
/// deliberately narrower than the device trends' mid-dot, because this trend
/// renders inside a table text column.)
const fn history_trend_gap(mode: TuiGlyphMode) -> char {
    match mode {
        TuiGlyphMode::Unicode => ' ',
        TuiGlyphMode::Ascii => '_',
    }
}

/// The ramp character for one clamped normalized level (index always 0..=7)
/// under the selected glyph repertoire. Both repertoires index the shared
/// `sparkline` component's published ramps (`
/// SPARKLINE_BLOCKS` / `SPARKLINE_ASCII_BLOCKS`) — the
/// one ladder single-source — so the app-history trend and the device trends
/// carry the same level at the same index by construction.
const fn history_trend_block(mode: TuiGlyphMode, index: usize) -> char {
    match mode {
        TuiGlyphMode::Unicode => SPARKLINE_BLOCKS[index],
        TuiGlyphMode::Ascii => SPARKLINE_ASCII_BLOCKS[index],
    }
}

/// Project a bounded envelope of the complete persisted history window
/// (oldest→newest, `NaN` = a recording-downtime gap) onto a single-line trend
/// in the given terminal glyph repertoire. Min/max normalization uses the
/// finite samples only, so downtime gaps never flatten the shape, and a window
/// without a single finite sample renders the honest "collecting" text instead
/// of a fabricated trend. The Unicode repertoire keeps the historical block
/// ramp byte for byte; the ASCII repertoire paints the same normalized levels
/// through the shared [`SPARKLINE_ASCII_BLOCKS`] ladder at
/// paint time, so an ASCII-only
/// terminal reads a monotonic gradient straight from the renderer instead of
/// the collapsed output of the post-paint cell rewrite.
pub(super) fn history_trend_in(mode: TuiGlyphMode, samples: &[f32]) -> String {
    history_trend_with_width_in(mode, samples, SPARKLINE_MAX_SAMPLES)
}

/// The same complete-window trend inside an explicitly allocated terminal slot.
pub(super) fn history_trend_with_width_in(
    mode: TuiGlyphMode,
    samples: &[f32],
    width: usize,
) -> String {
    let bounded = lttb_gap_preserving(samples, width.clamp(1, 600));
    let samples = bounded.as_slice();
    let finite = samples.iter().copied().filter(|sample| sample.is_finite());
    let min = finite.clone().fold(f32::INFINITY, f32::min);
    let max = finite.fold(f32::NEG_INFINITY, f32::max);
    if !min.is_finite() || !max.is_finite() {
        return t("history.application.collecting").to_owned();
    }
    samples
        .iter()
        .map(|sample| {
            if !sample.is_finite() {
                return history_trend_gap(mode);
            }
            // A flat series keeps its mid-ramp block; the projection answers it.
            let normalized = interval_fraction(*sample, min, max, FLAT_SERIES_SHOWS_MID_RAMP);
            history_trend_block(mode, ((normalized * 7.0).round() as usize).min(7))
        })
        .collect()
}

/// The explicit renderer mode for the shared sparkline component.
///
/// `Unicode` and `Ascii` preserve the established one-cell ramp exactly.
/// `Braille16` is an opt-in high-resolution path: each terminal cell is a pair
/// of Unicode Braille patterns, giving a 2×8 dot field (sixteen addressable
/// dots) across two text rows. No caller is forced into this mode, and the old
/// callers keep their byte-for-byte output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TuiSparklineMode {
    /// Existing half-block ramp.
    #[default]
    Unicode,
    /// Existing all-ASCII ink ladder.
    Ascii,
    /// Two stacked Unicode Braille cells per terminal column (2×8 dots).
    Braille16,
}

/// The painted result of one [`TuiSparklineMode`].
///
/// The legacy repertoires remain one line; the Braille path returns two
/// equal-width lines. Callers may keep a one-cell trend where an ASCII or
/// Unicode ramp is required, and may switch only a dedicated curve slot to the
/// two-row Braille form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TuiSparklineCurve {
    /// One existing Unicode/ASCII line.
    Single(String),
    /// Two stacked Braille lines. Their lengths are always equal.
    Braille16 {
        /// The upper 2×4 half of each Braille cell.
        upper: String,
        /// The lower 2×4 half of each Braille cell.
        lower: String,
    },
}

impl TuiSparklineCurve {
    /// The first painted row, or the only row for a legacy curve.
    #[must_use]
    pub fn upper(&self) -> &str {
        match self {
            Self::Single(text) => text,
            Self::Braille16 { upper, .. } => upper,
        }
    }

    /// The second Braille row; `None` for the existing one-row repertoires.
    #[must_use]
    pub fn lower(&self) -> Option<&str> {
        match self {
            Self::Single(_) => None,
            Self::Braille16 { lower, .. } => Some(lower),
        }
    }

    /// The common cell width of the curve.
    #[must_use]
    pub fn width(&self) -> usize {
        self.upper().chars().count()
    }
}

const BRAILLE_BASE_CODEPOINT: u32 = 0x2800;
const BRAILLE_BASE_CHAR: char = '\u{2800}';
const BRAILLE_LEVELS: usize = 8;
const BRAILLE_MID_LEVEL: usize = 4;

/// `(left, right)` dot masks for the four rows of one Braille cell, top row
/// first. Each row pair is one vertical level of the two-row 2×8 field.
const BRAILLE_ROW_DOTS: [(u8, u8); 4] = [
    (0x01, 0x08), // dots 1 and 4
    (0x02, 0x10), // dots 2 and 5
    (0x04, 0x20), // dots 3 and 6
    (0x40, 0x80), // dots 7 and 8
];

/// Render one sample sequence through the selected sparkline repertoire.
///
/// This is the public component entry point for new opt-in callers. The
/// Unicode and ASCII branches deliberately call the established `_in` family,
/// so their output is unchanged. Braille normalizes only finite samples,
/// paints one dot for each finite sample, and never turns a non-finite sample
/// into a fabricated point.
#[must_use]
pub fn render_sparkline(mode: TuiSparklineMode, samples: &[f32]) -> TuiSparklineCurve {
    match mode {
        TuiSparklineMode::Unicode => {
            TuiSparklineCurve::Single(sparkline_in(crate::TuiGlyphMode::Unicode, samples))
        }
        TuiSparklineMode::Ascii => {
            TuiSparklineCurve::Single(sparkline_in(crate::TuiGlyphMode::Ascii, samples))
        }
        TuiSparklineMode::Braille16 => braille16_sparkline(samples),
    }
}

/// Render the explicit 2×8 (sixteen-dot) Braille path as two rows.
#[must_use]
pub fn braille16_sparkline(samples: &[f32]) -> TuiSparklineCurve {
    let finite: Vec<(usize, f32)> = samples
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.is_finite().then_some((index, *value)))
        .collect();
    let Some((last_index, _)) = finite.last().copied() else {
        return TuiSparklineCurve::Braille16 {
            upper: String::new(),
            lower: String::new(),
        };
    };

    let min = finite
        .iter()
        .map(|(_, value)| *value)
        .fold(f32::INFINITY, f32::min);
    let max = finite
        .iter()
        .map(|(_, value)| *value)
        .fold(f32::NEG_INFINITY, f32::max);
    let cells = (last_index / 2) + 1;
    let mut upper = vec![0_u8; cells];
    let mut lower = vec![0_u8; cells];

    for (index, value) in finite {
        // A measured series with no travel keeps the Braille mid level, which is
        // the answer this surface declares.
        let normalized = interval_fraction(value, min, max, FLAT_SERIES_SHOWS_MID_RAMP);
        let level =
            ((normalized * (BRAILLE_LEVELS as f32 - 1.0)).round() as usize).min(BRAILLE_LEVELS - 1);
        let row = 3 - (level % 4);
        let left = index % 2 == 0;
        let dot = if left {
            BRAILLE_ROW_DOTS[row].0
        } else {
            BRAILLE_ROW_DOTS[row].1
        };
        if level >= BRAILLE_MID_LEVEL {
            upper[index / 2] |= dot;
        } else {
            lower[index / 2] |= dot;
        }
    }

    TuiSparklineCurve::Braille16 {
        upper: braille_row(&upper),
        lower: braille_row(&lower),
    }
}

/// Convert eight-dot masks to Unicode Braille characters. Every mask is a
/// valid codepoint offset, and zero intentionally renders the Braille blank
/// (`U+2800`) rather than a space so the two-row field retains its width.
fn braille_row(masks: &[u8]) -> String {
    masks
        .iter()
        .map(|&mask| {
            char::from_u32(BRAILLE_BASE_CODEPOINT + u32::from(mask)).unwrap_or(BRAILLE_BASE_CHAR)
        })
        .collect()
}
