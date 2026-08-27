//! Shared layout primitives, select option tables, and value mappers for
//! the grouped Iced Settings surface.
//!
//! Every control here consumes the owned inputs component family
//! (`crate::ui::components`): segmented choice groups, switches, sliders,
//! and selects. The pure `*_value` / `*_for_value` mappers are the single
//! seam between a control callback (a plain `usize`/`f32`/option value) and
//! the sanctioned [`SettingsChange`] vocabulary — the same messages the
//! legacy pill rows published, so persistence semantics cannot drift.

use std::fmt;
use std::ops::RangeInclusive;
use std::sync::OnceLock;

use iced::Length;
use iced::widget::{column, container, row, text};
use taskmanager_theme::tokens::MotionPolicy;
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{FontChoice, Skin, Theme, tokens};

use super::*;
use crate::app::ModeChoice;
use crate::ui::components::divider;

/// Label column width of one settings row (px contract): a stable left rail
/// for labels inside the modal panel's fixed 680px body, controls take the
/// remaining width.
const ROW_LABEL_WIDTH: f32 = 180.0;

/// The "persisted token names no real choice" sentinel handed to the
/// segmented control. The component marks no segment active and disables
/// arrow moves until the caller's state names a real choice — an unknown
/// token stays visibly unselected instead of being coerced onto a segment.
pub(super) const SEGMENT_NONE: usize = usize::MAX;

// ── layout primitives ────────────────────────────────────────────────────────

/// One semantic group header (GPUI parity): strong title over the shared
/// hairline divider, separating the Zed-style General / Appearance / Fonts /
/// System / Notifications / Units blocks.
pub(super) fn group_header<'a>(theme_snapshot: &'a Theme, title: &'static str) -> IcedElement<'a> {
    column(vec![
        text(title).size(f32::from(tokens::FONT_HEADER)).into(),
        divider(theme_snapshot),
    ])
    .spacing(f32::from(tokens::SPACE_4))
    .into()
}

/// A dim caption inside one group (the shared titled-section grammar every
/// Settings row hangs under).
pub(super) fn section_caption<'a>(
    theme_snapshot: &'a Theme,
    label: &'static str,
) -> IcedElement<'a> {
    text(label)
        .size(f32::from(tokens::FONT_CAPTION))
        .color(crate::theme::muted_text_color(theme_snapshot))
        .into()
}

/// One settings row: fixed-width label rail on the left, the control (or
/// static value) on the right.
pub(super) fn setting_row<'a>(label: &'static str, control: IcedElement<'a>) -> IcedElement<'a> {
    row![
        text(label)
            .size(f32::from(tokens::FONT_13))
            .width(Length::Fixed(ROW_LABEL_WIDTH)),
        control,
    ]
    .spacing(f32::from(tokens::SPACE_8))
    .padding([f32::from(tokens::SPACE_4), 0.0])
    .align_y(iced::Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// A muted caption line under one row (hint copy from the shared catalog).
pub(super) fn hint_line<'a>(theme_snapshot: &'a Theme, note: &'static str) -> IcedElement<'a> {
    text(note)
        .size(f32::from(tokens::FONT_CAPTION))
        .color(crate::theme::muted_text_color(theme_snapshot))
        .into()
}

/// A muted, non-interactive value: the honest display for a setting this
/// frontend cannot control (no fake toggle, no dead selector).
pub(super) fn static_value<'a>(theme_snapshot: &'a Theme, value: &'static str) -> IcedElement<'a> {
    text(value)
        .size(f32::from(tokens::FONT_13))
        .color(crate::theme::muted_text_color(theme_snapshot))
        .into()
}

/// Constrain one control's horizontal footprint (px contract for compact
/// selects inside the Fill control rail).
pub(super) fn boxed<'a>(width: f32, control: IcedElement<'a>) -> IcedElement<'a> {
    container(control).width(Length::Fixed(width)).into()
}

// ── skin segmented (GNOME / KDE / Windows / macOS) ───────────────────────────

pub(super) fn skin_choices() -> Vec<(String, usize)> {
    Skin::ALL
        .into_iter()
        .enumerate()
        .map(|(index, skin)| (skin.label().to_string(), index))
        .collect()
}

/// The active segment for a persisted skin token; unknown tokens name no
/// segment (legacy pill parity: no pill lit).
pub(super) fn skin_value(token: &str) -> usize {
    Skin::ALL
        .iter()
        .position(|skin| token.eq_ignore_ascii_case(skin.label()))
        .unwrap_or(SEGMENT_NONE)
}

pub(super) fn skin_for_value(value: usize) -> Skin {
    Skin::ALL.get(value).copied().unwrap_or(Skin::Gnome)
}

// ── mode segmented (Light / Dark / EyeForest / System) ──────────────────────

pub(super) fn mode_choices(language: Language) -> Vec<(String, usize)> {
    [
        i18n::t(language, Key::Light),
        i18n::t(language, Key::Dark),
        i18n::t(language, Key::EyeForest),
        i18n::t(language, Key::System),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, label)| (label.to_string(), index))
    .collect()
}

/// The active segment for a persisted mode token. The empty first-launch
/// sentinel behaves like `System` (legacy pill parity); unknown tokens name
/// no segment.
pub(super) fn mode_value(token: &str) -> usize {
    match token.to_ascii_lowercase().as_str() {
        "light" => 0,
        "dark" => 1,
        "eyeforest" | "eye-forest" => 2,
        "" | "system" => 3,
        _ => SEGMENT_NONE,
    }
}

pub(super) fn mode_for_value(value: usize) -> ModeChoice {
    match value {
        0 => ModeChoice::Light,
        1 => ModeChoice::Dark,
        2 => ModeChoice::EyeForest,
        _ => ModeChoice::System,
    }
}

// ── interface-size segmented (Small / Standard / Large) ─────────────────────

pub(super) fn ui_size_choices() -> Vec<(String, usize)> {
    [
        "settings.ui_size_small",
        "settings.ui_size_standard",
        "settings.ui_size_large",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, key)| (taskmanager_application::i18n::t(key).to_string(), index))
    .collect()
}

pub(super) fn ui_size_value(current: UiSize) -> usize {
    UiSize::ALL
        .iter()
        .position(|size| *size == current)
        .unwrap_or(1)
}

pub(super) fn ui_size_for_value(value: usize) -> UiSize {
    UiSize::ALL.get(value).copied().unwrap_or(UiSize::Standard)
}

// ── motion segmented (Normal / Reduced / None) ──────────────────────────────
// The segments mirror the shared `MotionPolicy` axis; the persisted token
// vocabulary is the core `MOTION_*` set ("normal" / "reduced" / "none").

pub(super) fn motion_choices() -> Vec<(String, usize)> {
    [
        "settings.motion_normal",
        "settings.motion_reduced",
        "settings.motion_none",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, key)| (taskmanager_application::i18n::t(key).to_string(), index))
    .collect()
}

/// The active segment for a persisted motion token. The empty sentinel and
/// unknown tokens light the Normal segment — the policy the snapshot seam
/// actually installs for them — so the control shows what is really in
/// effect, never a coerced-looking selection.
pub(super) fn motion_value(token: &str) -> usize {
    match token.trim().to_ascii_lowercase().as_str() {
        "reduced" => 1,
        "none" | "no-motion" => 2,
        _ => 0,
    }
}

pub(super) fn motion_for_value(value: usize) -> MotionPolicy {
    match value {
        1 => MotionPolicy::Reduced,
        2 => MotionPolicy::NoMotion,
        _ => MotionPolicy::Normal,
    }
}

// ── density segmented (Comfortable / Compact) ────────────────────────────────

pub(super) fn density_choices(language: Language) -> Vec<(String, usize)> {
    [
        i18n::t(language, Key::Comfortable),
        i18n::t(language, Key::Compact),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, label)| (label.to_string(), index))
    .collect()
}

/// `1` = the Compact segment (the persisted `"Compact"` token).
pub(super) fn density_value(compact: bool) -> usize {
    if compact { 1 } else { 0 }
}

// ── font-source segmented (System / Bundled) ────────────────────────────────

pub(super) fn font_choice_choices(language: Language) -> Vec<(String, usize)> {
    [
        i18n::t(language, Key::System),
        i18n::t(language, Key::Bundled),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, label)| (label.to_string(), index))
    .collect()
}

/// The active segment for a persisted font token: system marker → 0,
/// bundled/empty token → 1, a custom installed family names no segment
/// (the family select below carries the real choice).
pub(super) fn font_choice_value(is_system: bool, is_bundled: bool) -> usize {
    if is_system {
        0
    } else if is_bundled {
        1
    } else {
        SEGMENT_NONE
    }
}

pub(super) fn font_choice_for_value(value: usize) -> FontChoice {
    if value == 0 {
        FontChoice::System
    } else {
        FontChoice::Bundled
    }
}

// ── unit segmented pairs (Bytes / Bits · Base 2 / Base 10) ──────────────────

pub(super) fn unit_bytes_choices(language: Language) -> Vec<(String, usize)> {
    [i18n::t(language, Key::Bytes), i18n::t(language, Key::Bits)]
        .into_iter()
        .enumerate()
        .map(|(index, label)| (label.to_string(), index))
        .collect()
}

pub(super) fn unit_base_choices() -> Vec<(String, usize)> {
    [
        taskmanager_application::i18n::t("settings.base_2"),
        taskmanager_application::i18n::t("settings.base_10"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, label)| (label.to_string(), index))
    .collect()
}

/// The active segment of one two-state unit axis: `0` = the first (Bytes /
/// Base 2) segment, `1` = the second (Bits / Base 10).
pub(super) fn unit_toggle_value(first: bool) -> usize {
    if first { 0 } else { 1 }
}

// ── refresh-interval slider (0.5..=5.0 s on a 0.1 s grid) ───────────────────

/// Refresh slider floor (GPUI parity; the shell's telemetry clamp allows
/// 100 ms..=60 s, so every slider value is inside policy).
pub(super) const REFRESH_MIN_S: f32 = 0.5;
/// Refresh slider ceiling.
pub(super) const REFRESH_MAX_S: f32 = 5.0;
/// Refresh slider step. Every legacy pill interval (0.5 / 1 / 2 / 5 s) is an
/// exact grid point, so the old choosable values stay precisely reachable.
pub(super) const REFRESH_STEP_S: f32 = 0.1;

pub(super) fn refresh_range() -> RangeInclusive<f32> {
    REFRESH_MIN_S..=REFRESH_MAX_S
}

/// The persisted `refresh_ms` token for one slider position. Values come
/// from the slider's snapped grid, so the millis count is exact.
pub(super) fn refresh_value_to_ms(value: f32) -> u64 {
    (value * 1000.0).round() as u64
}

pub(super) fn refresh_label(value: f32) -> String {
    format!("{value:.1} s")
}

// ── graph data-points slider (10..=600 on a 10-point grid) ──────────────────

/// Data-points slider floor (the shared history-store clamp lower bound).
pub(super) const GRAPH_POINTS_MIN: f32 = 10.0;
/// Data-points slider ceiling (the shared clamp upper bound).
pub(super) const GRAPH_POINTS_MAX: f32 = 600.0;
/// Data-points slider step. Every legacy pill value (10 / 60 / 120 / 300 /
/// 600) is an exact grid point.
pub(super) const GRAPH_POINTS_STEP: f32 = 10.0;

pub(super) fn graph_points_range() -> RangeInclusive<f32> {
    GRAPH_POINTS_MIN..=GRAPH_POINTS_MAX
}

/// The persisted `graph_data_points` token for one slider position.
pub(super) fn graph_points_for_value(value: f32) -> usize {
    value.round() as usize
}

pub(super) fn graph_points_label(value: f32) -> String {
    format!("{value:.0}")
}

// ── quiet-hours select options (00:00 … 23:00) ─────────────────────────────

/// One quiet-hours hour choice; the `HH:00` label is locale-neutral digits
/// (GPUI parity).
#[derive(Clone, PartialEq)]
pub(super) struct QuietHour(pub u8);

impl fmt::Display for QuietHour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:00", self.0)
    }
}

/// The 24 quiet-hours options. The table is pure and deterministic, so the
/// process-wide cache can never go stale; `select` needs options that
/// outlive one render frame.
pub(super) fn quiet_hours() -> &'static [QuietHour] {
    static HOURS: OnceLock<Vec<QuietHour>> = OnceLock::new();
    HOURS.get_or_init(|| (0..24).map(QuietHour).collect())
}

// ── language select options ─────────────────────────────────────────────────

/// One language choice in its own tongue (`English` / `中文`) — the universal
/// language-picker convention; the label never translates.
#[derive(Clone, PartialEq)]
pub(super) struct LanguageChoice(pub Language);

impl fmt::Display for LanguageChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label())
    }
}

pub(super) fn language_choices() -> &'static [LanguageChoice; 2] {
    static CHOICES: [LanguageChoice; 2] =
        [LanguageChoice(Language::En), LanguageChoice(Language::Zh)];
    &CHOICES
}

// ── startup-page select options ─────────────────────────────────────────────

/// One startup-page choice: the persisted token plus its localized label.
#[derive(Clone, PartialEq)]
pub(super) struct StartupChoice {
    pub(super) token: &'static str,
    label: &'static str,
}

impl fmt::Display for StartupChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label)
    }
}

/// The three startup-page options for one language. Labels resolve through
/// the pure `i18n::t(language, key)` table, so the per-language cache is
/// deterministic and never stale; `select` needs options that outlive one
/// render frame.
pub(super) fn startup_choices(language: Language) -> &'static [StartupChoice] {
    static EN: OnceLock<Vec<StartupChoice>> = OnceLock::new();
    static ZH: OnceLock<Vec<StartupChoice>> = OnceLock::new();
    fn build(language: Language) -> Vec<StartupChoice> {
        vec![
            StartupChoice {
                token: "",
                label: i18n::t(language, Key::RememberLast),
            },
            StartupChoice {
                token: "performance",
                label: i18n::t(language, Key::Performance),
            },
            StartupChoice {
                token: "apps",
                label: i18n::t(language, Key::Applications),
            },
        ]
    }
    match language {
        Language::En => EN.get_or_init(|| build(Language::En)),
        Language::Zh => ZH.get_or_init(|| build(Language::Zh)),
    }
}

/// The selected startup-page choice for a persisted token; an unknown token
/// selects nothing (legacy pill parity: no pill lit).
pub(super) fn startup_selected<'a>(
    choices: &'a [StartupChoice],
    token: &str,
) -> Option<&'a StartupChoice> {
    choices.iter().find(|choice| choice.token == token)
}
