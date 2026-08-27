//! Settings overlay: a keyboard-driven preferences form.
//!
//! The form owns seven fields (skin × 4, mode × 4, high contrast, UI font,
//! mono font, density, language). Navigation is Tab / arrow keys, values
//! change with Left/Right, Enter submits a client-local patch through the
//! background configuration coordinator, and Esc cancels. The language
//! choice persists through `Config::language` (`"en"`/`"zh"` tokens, G-22):
//! saving writes the token and applies it to the process-global i18n bundle,
//! and the composition-edge restore re-applies it at startup.
//!
//! The persisted tokens are the same opaque strings the GPUI frontend reads
//! (`Skin::label`, `"Light"`/`"Dark"`/`"EyeForest"`/`"System"`, `""`/`"MiSans VF"`/
//! `"Roboto Mono"`, `"Comfortable"`/`"Compact"`), so a preference set in
//! the terminal applies to the graphical frontend too.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use taskmanager_application::Config;
use taskmanager_application::i18n::t;
use taskmanager_theme::Skin;
use taskmanager_ui_contract::IconId;

use crate::ThemeParams;
use crate::TuiTheme;

/// Number of settings fields (row count of the form): 0 skin, 1 mode,
/// 2 high contrast, 3 UI font, 4 mono font, 5 density, 6 language,
/// 7 refresh interval, 8..18 device visibility (10 families), 18..24 unit
/// matrix (3 families × bytes/base2), 24 gray-zero-values, 25 graph points,
/// 26 desktop notifications, 27..28 quiet-hours start/end (hours),
/// 29 continuous history recording.
pub const SETTINGS_FIELDS: usize = 30;

/// Quiet-hours hour range (inclusive selection 0..=23); start == end means
/// "no quiet hours" (the gate treats equal bounds as never-suppressing).
pub const QUIET_HOURS_MAX: u8 = 24;

/// The unit-matrix families in field order: (use_bytes, use_base2) per family.
pub const UNIT_FAMILIES: [&str; 3] = ["memory", "drive", "network"];

const DEVICE_LABEL_KEYS: [&str; 10] = [
    "settings.show_cpu",
    "settings.show_memory",
    "settings.show_disks",
    "settings.show_network",
    "settings.show_network_wired",
    "settings.show_network_wireless",
    "settings.show_network_vpn",
    "settings.show_network_virtual",
    "settings.show_network_other",
    "settings.show_gpus",
];

/// The telemetry refresh-interval choices (ms), in display order.
pub const REFRESH_MS: [u64; 4] = [500, 1_000, 2_000, 5_000];
pub const REFRESH_LABELS: [&str; 4] = ["0.5 s", "1 s", "2 s", "5 s"];

/// The Performance sparkline window choices (samples), in display order
/// (GPUI's 10..=600 range as discrete steps; 60 is the historical default).
pub const GRAPH_POINTS: [usize; 4] = [60, 120, 300, 600];
pub const GRAPH_LABELS: [&str; 4] = ["60", "120", "300", "600"];

const FONT_SYSTEM: &str = "";
const FONT_MISANS: &str = "MiSans VF";
const FONT_ROBOTO_MONO: &str = "Roboto Mono";
const FONT_LABELS: [&str; 3] = ["System", "MiSans VF", "Roboto Mono"];
const DENSITY_TOKENS: [&str; 2] = ["Comfortable", "Compact"];
const MODE_TOKENS: [&str; 4] = ["Light", "Dark", "EyeForest", "System"];
/// The persisted `Config::language` token for each language index (G-22).
/// Parallel to [`LANGUAGE_TOKENS`] and driven by the same index so the form
/// choice and the persisted token can never drift apart.
const LANGUAGE_TOKENS: [&str; 2] = ["en", "zh"];

/// Keyboard-navigable settings form state. Field indices are stable:
/// 0 skin, 1 mode, 2 high contrast, 3 UI font, 4 mono font, 5 density,
/// 6 language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsForm {
    /// Focused field index (0..7); the order is documented on the type.
    pub field: usize,
    /// Index into [`Skin::ALL`].
    pub skin: usize,
    /// Index into the product-first mode choices (Light / Dark / EyeForest),
    /// with System retained as the secondary native-chrome option.
    pub mode: usize,
    pub hc: bool,
    /// Index into the UI font choices.
    pub ui_font: usize,
    pub mono_font: usize,
    /// Index into the density choices.
    pub density: usize,
    /// Index into the language choices (parallel to the `LANGUAGE_TOKENS`
    /// table). Persisted through `Config::language` and re-applied at the
    /// composition edge (G-22); an unrecorded preference keeps the
    /// host-detected locale.
    pub language: usize,
    /// Index into REFRESH_MS (the telemetry refresh interval).
    pub refresh: usize,
    /// Device-family visibility toggles (field 8..18, order = DEVICE_FIELDS).
    pub show: [bool; 10],
    /// Unit-matrix toggles (field 18..24): [memory_bytes, memory_base2,
    /// drive_bytes, drive_base2, network_bytes, network_base2].
    pub units: [bool; 6],
    /// Gray-out zero values on the process table (field 24).
    pub gray_zero: bool,
    /// Index into GRAPH_POINTS (field 25, the Performance sparkline window).
    pub graph_points: usize,
    /// Desktop notifications opt-in (field 26, BN-07).
    pub notify_enabled: bool,
    /// Quiet-hours start hour 0..=23 (field 27); equal start/end = none.
    pub quiet_start: u8,
    /// Quiet-hours end hour 0..=23 (field 28); equal start/end = none.
    pub quiet_end: u8,
    /// Continuous background collection and durable history opt-in (field 29).
    pub history_persistence: bool,
    /// Last save failure detail (rendered in the overlay footer).
    pub save_error: Option<String>,
}

impl Default for SettingsForm {
    fn default() -> Self {
        Self {
            field: 0,
            skin: 0,
            mode: 1,
            hc: false,
            ui_font: 0,
            mono_font: 0,
            density: 0,
            language: 0,
            refresh: 1,
            show: [true; 10],
            units: [true, true, true, true, false, false],
            gray_zero: false,
            graph_points: 1,
            // Notifications default OFF (opt-in); no quiet hours by default.
            notify_enabled: false,
            quiet_start: 0,
            quiet_end: 0,
            history_persistence: false,
            save_error: None,
        }
    }
}

impl SettingsForm {
    /// Seed the form from the opaque config tokens (empty/unknown tokens keep
    /// the built-in defaults). `language` is seeded separately through
    /// [`Self::language_index_for`] at the composition edge (the same pattern
    /// the visibility/unit arrays use).
    #[must_use]
    pub fn from_config_tokens(
        skin: &str,
        mode: &str,
        hc: bool,
        ui_font: &str,
        mono_font: &str,
        density: &str,
        notifications: (bool, Option<(u16, u16)>),
    ) -> Self {
        let skin = Skin::ALL
            .into_iter()
            .position(|candidate| candidate.label().eq_ignore_ascii_case(skin))
            .unwrap_or(0);
        let mode = match mode {
            "Light" => 0,
            "Dark" => 1,
            "EyeForest" => 2,
            "System" => 3,
            _ => 1,
        };
        let ui_font = FONT_LABELS
            .iter()
            .position(|label| font_token_for(label) == ui_font)
            .unwrap_or(0);
        let mono_font = FONT_LABELS
            .iter()
            .position(|label| font_token_for(label) == mono_font)
            .unwrap_or(0);
        let density = DENSITY_TOKENS
            .iter()
            .position(|label| *label == density)
            .unwrap_or(0);
        let (notify_enabled, notify_quiet_hours) = notifications;
        let (quiet_start, quiet_end) = notify_quiet_hours
            .map(|(start, end)| {
                (
                    u8::try_from(start / 60).unwrap_or(0),
                    u8::try_from(end / 60).unwrap_or(0),
                )
            })
            .unwrap_or((0, 0));
        Self {
            field: 0,
            skin,
            mode,
            hc,
            ui_font,
            mono_font,
            density,
            language: 0,
            refresh: 1,
            show: [true; 10],
            units: [true, true, true, true, false, false],
            gray_zero: false,
            graph_points: 1,
            notify_enabled,
            quiet_start,
            quiet_end,
            history_persistence: false,
            save_error: None,
        }
    }

    /// The notification policy for the current form state (BN-07). Equal
    /// quiet hours mean "no quiet hours".
    #[must_use]
    pub fn notification_policy(&self) -> taskmanager_application::alerts::NotificationPolicy {
        taskmanager_application::alerts::NotificationPolicy {
            enabled: self.notify_enabled,
            cooldown_ms: taskmanager_application::alerts::NotificationPolicy::default().cooldown_ms,
            quiet_hours: (self.quiet_start != self.quiet_end).then(|| {
                taskmanager_application::alerts::QuietHours {
                    start_minutes: u16::from(self.quiet_start) * 60,
                    end_minutes: u16::from(self.quiet_end) * 60,
                }
            }),
        }
    }

    /// The selected Performance sparkline window (samples).
    #[must_use]
    pub const fn graph_points(&self) -> usize {
        GRAPH_POINTS[self.graph_points]
    }

    /// The persisted refresh interval (ms) for the current selection.
    #[must_use]
    pub const fn refresh_ms(&self) -> u64 {
        REFRESH_MS[self.refresh]
    }

    /// Move focus to the next (delta +1) or previous (delta -1) field,
    /// wrapping at the edges.
    pub fn move_field(&mut self, delta: isize) {
        self.field = self
            .field
            .saturating_add_signed(delta)
            .min(SETTINGS_FIELDS - 1);
    }

    /// Step the focused field's value by `delta` (wrapping). The high-contrast
    /// toggle flips on any step.
    pub fn step_value(&mut self, delta: isize) {
        let wrap = |current: usize, delta: isize, len: usize| {
            let len = len as isize;
            let next = (current as isize + delta).rem_euclid(len);
            next as usize
        };
        match self.field {
            0 => self.skin = wrap(self.skin, delta, Skin::ALL.len()),
            1 => self.mode = wrap(self.mode, delta, MODE_TOKENS.len()),
            2 => self.hc = !self.hc,
            3 => self.ui_font = wrap(self.ui_font, delta, FONT_LABELS.len()),
            4 => self.mono_font = wrap(self.mono_font, delta, FONT_LABELS.len()),
            5 => self.density = wrap(self.density, delta, DENSITY_TOKENS.len()),
            6 => self.language = wrap(self.language, delta, LANGUAGE_TOKENS.len()),
            7 => self.refresh = wrap(self.refresh, delta, REFRESH_MS.len()),
            8..=17 => {
                let index = self.field - 8;
                self.show[index] = !self.show[index];
            }
            18..=23 => {
                let index = self.field - 18;
                self.units[index] = !self.units[index];
            }
            24 => self.gray_zero = !self.gray_zero,
            25 => self.graph_points = wrap(self.graph_points, delta, GRAPH_POINTS.len()),
            26 => self.notify_enabled = !self.notify_enabled,
            27 => {
                self.quiet_start = wrap(
                    usize::from(self.quiet_start),
                    delta,
                    usize::from(QUIET_HOURS_MAX),
                ) as u8
            }
            28 => {
                self.quiet_end = wrap(
                    usize::from(self.quiet_end),
                    delta,
                    usize::from(QUIET_HOURS_MAX),
                ) as u8
            }
            29 => self.history_persistence = !self.history_persistence,
            _ => {}
        }
    }

    /// The persisted skin token for the current selection.
    #[must_use]
    pub fn skin_token(&self) -> &'static str {
        Skin::ALL[self.skin].label()
    }

    /// The persisted mode token for the current selection.
    #[must_use]
    pub fn mode_token(&self) -> &'static str {
        MODE_TOKENS[self.mode]
    }

    /// The persisted UI-font token for the current selection.
    #[must_use]
    pub fn ui_font_token(&self) -> &'static str {
        font_token_for(FONT_LABELS[self.ui_font])
    }

    /// The persisted mono-font token for the current selection.
    #[must_use]
    pub fn mono_font_token(&self) -> &'static str {
        font_token_for(FONT_LABELS[self.mono_font])
    }

    /// The persisted density token for the current selection.
    #[must_use]
    pub const fn density_token(&self) -> &'static str {
        DENSITY_TOKENS[self.density]
    }

    /// The persisted language token for the current selection (`"en"`/`"zh"`,
    /// G-22). Written to `Config::language` on save and re-applied at startup.
    #[must_use]
    pub const fn language_token(&self) -> &'static str {
        LANGUAGE_TOKENS[self.language]
    }

    /// The form index for a persisted language token (`None`/unknown keeps
    /// the English default — the base of the zh→en→key fallback chain).
    #[must_use]
    pub fn language_index_for(token: Option<&str>) -> usize {
        token
            .and_then(|token| LANGUAGE_TOKENS.iter().position(|code| *code == token))
            .unwrap_or(0)
    }

    /// The i18n bundle language for a persisted token; `None` when no
    /// preference is recorded (the caller keeps the host-detected locale,
    /// per the `Config::language` contract).
    #[must_use]
    pub fn language_for_token(
        token: Option<&str>,
    ) -> Option<taskmanager_application::i18n::Language> {
        // Map through the parallel token table (single source with the form
        // indices) so an unknown spelling can never resolve to a language.
        let index = Self::language_index_for(token);
        token.map(|_| match index {
            1 => taskmanager_application::i18n::Language::Zh,
            _ => taskmanager_application::i18n::Language::En,
        })
    }
}

fn font_token_for(label: &str) -> &'static str {
    match label {
        "MiSans VF" => FONT_MISANS,
        "Roboto Mono" => FONT_ROBOTO_MONO,
        _ => FONT_SYSTEM,
    }
}

/// Apply the form to a client-local config draft and to the runtime theme
/// parameters. The caller submits the draft through the background
/// coordinator; this pure projection performs no filesystem I/O.
/// The language choice persists through `Config::language` (G-22); the caller
/// re-applies the i18n bundle on save.
pub fn apply_settings_to_config(
    form: &SettingsForm,
    config: &mut Config,
    theme_params: &mut ThemeParams,
) {
    config.skin = form.skin_token().into();
    config.mode = form.mode_token().into();
    config.hc = form.hc;
    config.ui_font = form.ui_font_token().into();
    config.mono_font = form.mono_font_token().into();
    config.density = form.density_token().into();
    config.language = Some(form.language_token().into());
    config.refresh_ms = form.refresh_ms();
    config.show_cpu = form.show[0];
    config.show_memory = form.show[1];
    config.show_disks = form.show[2];
    config.show_network = form.show[3];
    config.show_network_wired = form.show[4];
    config.show_network_wireless = form.show[5];
    config.show_network_vpn = form.show[6];
    config.show_network_virtual = form.show[7];
    config.show_network_other = form.show[8];
    config.show_gpus = form.show[9];
    config.memory_use_bytes = form.units[0];
    config.memory_use_base2 = form.units[1];
    config.drive_use_bytes = form.units[2];
    config.drive_use_base2 = form.units[3];
    config.network_use_bytes = form.units[4];
    config.network_use_base2 = form.units[5];
    config.gray_zero_values = form.gray_zero;
    config.graph_data_points = form.graph_points() as u32;
    // Single-source policy mapping (BN-07): the persisted fields mirror the
    // form's quiet hours; cooldown stays on the built-in default.
    let policy = form.notification_policy();
    config.apply_notification_policy(&policy);
    config.history_persistence = form.history_persistence;
    *theme_params = ThemeParams::from_config_tokens(&config.skin, &config.mode, config.hc);
}

/// Render the settings overlay centred over `area`. Does nothing if the
/// terminal is too small for a readable form.
pub fn render_settings_overlay(
    frame: &mut Frame<'_>,
    form: &SettingsForm,
    theme: TuiTheme,
    area: Rect,
) {
    let popup = centered(area, 68, 32);
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            crate::icon_glyph(IconId::Settings),
            t("chrome.settings")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body, footer] =
        Layout::vertical([Constraint::Min(10), Constraint::Length(4)]).areas(inner);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(SETTINGS_FIELDS + 1);
    lines.push(field_line(
        form,
        0,
        t("settings.skin"),
        Skin::ALL[form.skin].label(),
        theme,
    ));
    lines.push(field_line(
        form,
        1,
        t("settings.mode"),
        mode_display_label(form.mode),
        theme,
    ));
    lines.push(field_line(
        form,
        2,
        t("settings.high_contrast"),
        if form.hc {
            t("settings.on")
        } else {
            t("settings.off")
        },
        theme,
    ));
    lines.push(field_line(
        form,
        3,
        t("settings.desktop_ui_font"),
        font_display_label(form.ui_font),
        theme,
    ));
    lines.push(field_line(
        form,
        4,
        t("settings.desktop_mono_font"),
        font_display_label(form.mono_font),
        theme,
    ));
    lines.push(field_line(
        form,
        5,
        t("settings.row_density"),
        density_display_label(form.density),
        theme,
    ));
    lines.push(field_line(
        form,
        6,
        t("settings.language"),
        language_display_label(form.language),
        theme,
    ));
    lines.push(field_line(
        form,
        7,
        t("settings.refresh_interval"),
        REFRESH_LABELS[form.refresh],
        theme,
    ));
    for (index, key) in DEVICE_LABEL_KEYS.iter().enumerate() {
        lines.push(field_line(
            form,
            8 + index,
            t(key),
            if form.show[index] {
                t("settings.on")
            } else {
                t("settings.off")
            },
            theme,
        ));
    }
    for family_index in 0..UNIT_FAMILIES.len() {
        let bytes_on = form.units[family_index * 2];
        let base2_on = form.units[family_index * 2 + 1];
        lines.push(field_line(
            form,
            18 + family_index * 2,
            unit_label_key(family_index, true),
            if bytes_on {
                t("settings.bytes")
            } else {
                t("settings.bits")
            },
            theme,
        ));
        lines.push(field_line(
            form,
            19 + family_index * 2,
            unit_label_key(family_index, false),
            if base2_on {
                t("settings.base_2")
            } else {
                t("settings.base_10")
            },
            theme,
        ));
    }
    lines.push(field_line(
        form,
        24,
        t("settings.gray_zero_values"),
        if form.gray_zero {
            t("settings.on")
        } else {
            t("settings.off")
        },
        theme,
    ));
    lines.push(field_line(
        form,
        25,
        t("settings.graph_data_points"),
        GRAPH_LABELS[form.graph_points],
        theme,
    ));
    lines.push(field_line(
        form,
        26,
        t("settings.desktop_notifications"),
        if form.notify_enabled {
            t("settings.on")
        } else {
            t("settings.off")
        },
        theme,
    ));
    lines.push(field_line(
        form,
        27,
        t("settings.quiet_hours_start"),
        &format!("{:02}:00", form.quiet_start),
        theme,
    ));
    lines.push(field_line(
        form,
        28,
        t("settings.quiet_hours_end"),
        &format!("{:02}:00", form.quiet_end),
        theme,
    ));
    lines.push(field_line(
        form,
        29,
        t("settings.history_persistence"),
        if form.history_persistence {
            t("settings.on")
        } else {
            t("settings.off")
        },
        theme,
    ));
    // Scroll the focused field into view on short terminals (the body shows
    // roughly `body.height - 1` rows; a focused field past that window is
    // still reachable via Tab and becomes visible as the offset follows it).
    let visible_rows = body.height.saturating_sub(1) as usize;
    let scroll = form.field.saturating_sub(visible_rows.saturating_sub(1)) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((scroll, 0)),
        body,
    );

    let mut footer_spans: Vec<Span<'static>> = vec![
        Span::styled(
            format!(" {} ", t("tui.settings_move")),
            Style::new().fg(Color::Black).bg(theme.accent),
        ),
        Span::styled(
            format!("  {}", t("tui.settings_save")),
            Style::new().fg(theme.dim),
        ),
    ];
    if let Some(error) = form.save_error.as_deref() {
        footer_spans.push(Span::styled(
            format!("  ✗ {error}"),
            Style::new().fg(theme.danger),
        ));
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(footer_spans),
            Line::from(vec![Span::styled(
                t("settings.footer_hint"),
                Style::new().fg(theme.dim),
            )]),
            Line::from(vec![Span::styled(
                t("settings.font_terminal_hint"),
                Style::new().fg(theme.dim),
            )]),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

fn mode_display_label(index: usize) -> &'static str {
    match MODE_TOKENS.get(index).copied().unwrap_or("Dark") {
        "Light" => t("settings.light"),
        "EyeForest" => t("settings.eyeforest"),
        "System" => t("settings.system"),
        _ => t("settings.dark"),
    }
}

fn font_display_label(index: usize) -> &'static str {
    match FONT_LABELS.get(index).copied().unwrap_or("System") {
        "MiSans VF" => FONT_MISANS,
        "Roboto Mono" => FONT_ROBOTO_MONO,
        _ => t("settings.font_system"),
    }
}

fn density_display_label(index: usize) -> &'static str {
    match DENSITY_TOKENS.get(index).copied().unwrap_or("Comfortable") {
        "Compact" => t("settings.density_compact"),
        _ => t("settings.density_comfortable"),
    }
}

fn language_display_label(index: usize) -> &'static str {
    match LANGUAGE_TOKENS.get(index).copied().unwrap_or("en") {
        "zh" => t("settings.lang_zh"),
        _ => t("settings.lang_en"),
    }
}

fn unit_label_key(family_index: usize, bytes: bool) -> &'static str {
    match (family_index, bytes) {
        (0, true) => "settings.memory_usage_unit",
        (0, false) => "settings.memory_usage_base",
        (1, true) => "settings.drive_usage_unit",
        (1, false) => "settings.drive_usage_base",
        (2, true) => "settings.network_usage_unit",
        (2, false) => "settings.network_usage_base",
        _ => "settings.memory_usage_unit",
    }
}

fn field_line<'a>(
    form: &'a SettingsForm,
    field: usize,
    label: &'a str,
    value: &'a str,
    theme: TuiTheme,
) -> Line<'static> {
    let focused = form.field == field;
    let marker = if focused { "▸ " } else { "  " };
    Line::from(vec![
        Span::styled(
            format!("{marker}{label:<16}"),
            Style::new()
                .fg(if focused { theme.accent } else { theme.dim })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::styled(
            value.to_owned(),
            Style::new().fg(if focused { Color::White } else { theme.dim }),
        ),
    ])
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/settings_tests.rs"]
mod tests;
