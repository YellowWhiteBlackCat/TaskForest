//! Settings overlay: a keyboard-driven preferences form.
//!
//! The form owns seven fields (skin × 4, mode × 4, high contrast, UI font,
//! mono font, density, language). Navigation is Tab / arrow keys, choice and
//! bounded range values step with Left/Right, Space toggles boolean fields,
//! Enter submits a client-local patch through the background configuration
//! coordinator, and Esc cancels. The language
//! choice persists through `Config::language` (`"en"`/`"zh"` tokens, G-22):
//! saving writes the token and applies it to the process-global i18n bundle,
//! and the composition-edge restore re-applies it at startup.
//!
//! The persisted tokens are the theme owner's public vocabulary
//! ([`Skin::config_token`], [`ModePreference::config_token`],
//! [`FontChoice::config_token`], [`RowDensity::config_token`]) — the same
//! strings the other three products write — so a preference set in the terminal
//! applies to the graphical frontends unchanged. The form spells none of them.
//!
//! [`Skin::config_token`]: taskmanager_theme::Skin::config_token
//! [`ModePreference::config_token`]: taskmanager_theme::ModePreference::config_token
//! [`FontChoice::config_token`]: taskmanager_theme::FontChoice::config_token
//! [`RowDensity::config_token`]: taskmanager_theme::RowDensity::config_token

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use taskforest_core::core::config::Config;
use taskmanager_application::i18n::t;
use taskmanager_theme::tokens::RowDensity;
use taskmanager_theme::{FontAvailability, FontChoice, FontRole, LightDark, ModePreference, Skin};
use taskmanager_ui_contract::IconId;

use crate::ThemeParams;
use crate::TuiTheme;
use taskforest_core::core::alerts::{NotificationPolicy, QuietHours};
use taskmanager_application::i18n::Language;
mod footer;
mod layout;
use footer::settings_footer;
use layout::settings_field_rows;
pub(crate) use layout::{SettingsOverlayLayout, settings_overlay_layout};

/// Number of settings fields (row count of the form): 0 skin, 1 mode,
/// 2 high contrast, 3 UI font, 4 mono font, 5 density, 6 language,
/// 7 refresh interval, 8..18 device visibility (10 families), 18..24 unit
/// matrix (3 families × bytes/base2), 24 gray-zero-values, 25 graph points,
/// 26 desktop notifications, 27..28 quiet-hours start/end (hours),
/// 29 continuous history recording, 30..34 permission-center actions.
pub const SETTINGS_FIELDS: usize = 34;

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

use taskmanager_ui_contract::presentation::settings::{CAPACITY_CHOICES, REFRESH_CHOICES_MS};

pub const REFRESH_LABELS: [&str; 4] = ["0.5 s", "1 s", "2 s", "5 s"];

pub const GRAPH_LABELS: [&str; 4] = ["60", "120", "300", "600"];

/// The persisted `Config::language` token for each language index (G-22).
/// Parallel to [`LANGUAGE_TOKENS`] and driven by the same index so the form
/// choice and the persisted token can never drift apart.
const LANGUAGE_TOKENS: [&str; 2] = ["en", "zh"];

/// The TUI paints no glyphs of its own — the terminal emulator owns the face.
/// Its two font rows therefore persist the shared tokens for the desktop
/// products to read, and resolve them through an EMPTY host catalog: no family
/// the terminal did not observe can become primary here, and a custom name can
/// only ever fail closed to the skin default.
#[must_use]
fn terminal_font_availability() -> FontAvailability {
    FontAvailability::from_installed_families([])
}

/// The index of one typed axis member in the public ladder the form walks, so
/// a settings row and a config file never disagree about what index 2 means.
#[must_use]
fn skin_index(skin: Skin) -> usize {
    Skin::ALL
        .into_iter()
        .position(|candidate| candidate == skin)
        .unwrap_or(0)
}

#[must_use]
fn mode_index(preference: ModePreference) -> usize {
    ModePreference::CHOICES
        .into_iter()
        .position(|candidate| candidate == preference)
        .unwrap_or(0)
}

#[must_use]
fn density_index(density: RowDensity) -> usize {
    RowDensity::ALL
        .into_iter()
        .position(|candidate| candidate == density)
        .unwrap_or(0)
}

#[must_use]
fn font_index(choice: FontChoice) -> usize {
    FontChoice::PICKER_LADDER
        .iter()
        .position(|candidate| *candidate == choice)
        .unwrap_or(0)
}

/// Keyboard-navigable settings form state. Field indices are stable:
/// 0 skin, 1 mode, 2 high contrast, 3 UI font, 4 mono font, 5 density,
/// 6 language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsForm {
    /// Focused field index (0..7); the order is documented on the type.
    pub field: usize,
    /// Index into [`Skin::ALL`].
    pub skin: usize,
    /// Index into [`ModePreference::CHOICES`] (Light / Dark / EyeForest, then
    /// follow-native) — the picker ladder the theme owner publishes.
    pub mode: usize,
    pub hc: bool,
    /// Index into [`FontChoice::PICKER_LADDER`] for the UI role.
    pub ui_font: usize,
    /// Index into [`FontChoice::PICKER_LADDER`] for the monospace role.
    pub mono_font: usize,
    /// Index into [`RowDensity::ALL`].
    pub density: usize,
    /// Index into the language choices (parallel to the `LANGUAGE_TOKENS`
    /// table). Persisted through `Config::language` and re-applied at the
    /// composition edge (G-22); an unrecorded preference keeps the
    /// host-detected locale.
    pub language: usize,
    /// Index into `REFRESH_CHOICES_MS` (the telemetry refresh interval).
    pub refresh: usize,
    /// Device-family visibility toggles (field 8..18, order = DEVICE_FIELDS).
    pub show: [bool; 10],
    /// Unit-matrix toggles (field 18..24): [memory_bytes, memory_base2,
    /// drive_bytes, drive_base2, network_bytes, network_base2].
    pub units: [bool; 6],
    /// Gray-out zero values on the process table (field 24).
    pub gray_zero: bool,
    /// Index into `CAPACITY_CHOICES` (field 25, the Performance sparkline window).
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
            skin: skin_index(Skin::Gnome),
            mode: mode_index(ModePreference::default()),
            hc: false,
            // An unrecorded preference is the shipped product default: the
            // bundled faces, follow-native mode, comfortable rows.
            ui_font: font_index(FontChoice::default()),
            mono_font: font_index(FontChoice::default()),
            density: density_index(RowDensity::default()),
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
        let skin = skin_index(Skin::from_config_token(skin).unwrap_or(Skin::Gnome));
        let mode = mode_index(ModePreference::from_config_token(mode));
        let availability = terminal_font_availability();
        let ui_font = font_index(FontChoice::from_config_token(
            ui_font,
            FontRole::Ui,
            &availability,
        ));
        let mono_font = font_index(FontChoice::from_config_token(
            mono_font,
            FontRole::Mono,
            &availability,
        ));
        let density = density_index(RowDensity::from_config_token(density).unwrap_or_default());
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
    pub fn notification_policy(&self) -> NotificationPolicy {
        NotificationPolicy {
            enabled: self.notify_enabled,
            cooldown_ms: NotificationPolicy::default().cooldown_ms,
            quiet_hours: (self.quiet_start != self.quiet_end).then(|| QuietHours {
                start_minutes: u16::from(self.quiet_start) * 60,
                end_minutes: u16::from(self.quiet_end) * 60,
            }),
        }
    }

    /// The selected Performance sparkline window (samples).
    #[must_use]
    pub const fn graph_points(&self) -> usize {
        CAPACITY_CHOICES[self.graph_points]
    }

    /// The persisted refresh interval (ms) for the current selection.
    #[must_use]
    pub const fn refresh_ms(&self) -> u64 {
        REFRESH_CHOICES_MS[self.refresh]
    }

    /// Move focus to the next (delta +1) or previous (delta -1) field,
    /// wrapping at the edges.
    pub fn move_field(&mut self, delta: isize) {
        self.field = self
            .field
            .saturating_add_signed(delta)
            .min(SETTINGS_FIELDS - 1);
    }

    /// Whether the focused setting is a keyboard switch that accepts Space.
    /// Range and choice fields keep their Left/Right stepping contract, while
    /// permission-center rows are actions rather than switches.
    #[must_use]
    pub(crate) const fn focused_switch(&self) -> bool {
        matches!(self.field, 2 | 8..=24 | 26 | 29)
    }

    /// Step the focused field's value by `delta`. Discrete Slider adaptations
    /// clamp at their endpoints; enumerated choices continue to wrap, and
    /// boolean switches flip on any step.
    pub fn step_value(&mut self, delta: isize) {
        let wrap = |current: usize, delta: isize, len: usize| {
            let len = len as isize;
            let next = (current as isize + delta).rem_euclid(len);
            next as usize
        };
        let clamp = |current: usize, delta: isize, len: usize| {
            current
                .saturating_add_signed(delta)
                .min(len.saturating_sub(1))
        };
        match self.field {
            0 => self.skin = wrap(self.skin, delta, Skin::ALL.len()),
            1 => self.mode = wrap(self.mode, delta, ModePreference::CHOICES.len()),
            2 => self.hc = !self.hc,
            3 => self.ui_font = wrap(self.ui_font, delta, FontChoice::PICKER_LADDER.len()),
            4 => self.mono_font = wrap(self.mono_font, delta, FontChoice::PICKER_LADDER.len()),
            5 => self.density = wrap(self.density, delta, RowDensity::ALL.len()),
            6 => self.language = wrap(self.language, delta, LANGUAGE_TOKENS.len()),
            7 => self.refresh = clamp(self.refresh, delta, REFRESH_CHOICES_MS.len()),
            8..=17 => {
                let index = self.field - 8;
                self.show[index] = !self.show[index];
            }
            18..=23 => {
                let index = self.field - 18;
                self.units[index] = !self.units[index];
            }
            24 => self.gray_zero = !self.gray_zero,
            25 => self.graph_points = clamp(self.graph_points, delta, CAPACITY_CHOICES.len()),
            26 => self.notify_enabled = !self.notify_enabled,
            27 => {
                self.quiet_start = clamp(
                    usize::from(self.quiet_start),
                    delta,
                    usize::from(QUIET_HOURS_MAX),
                ) as u8
            }
            28 => {
                self.quiet_end = clamp(
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
        Skin::ALL[self.skin].config_token()
    }

    /// The persisted mode token for the current selection.
    #[must_use]
    pub fn mode_token(&self) -> &'static str {
        ModePreference::CHOICES[self.mode].config_token()
    }

    /// The persisted UI-font token for the current selection.
    #[must_use]
    pub fn ui_font_token(&self) -> &'static str {
        FontChoice::PICKER_LADDER[self.ui_font].config_token(FontRole::Ui)
    }

    /// The persisted mono-font token for the current selection.
    #[must_use]
    pub fn mono_font_token(&self) -> &'static str {
        FontChoice::PICKER_LADDER[self.mono_font].config_token(FontRole::Mono)
    }

    /// The persisted density token for the current selection.
    #[must_use]
    pub const fn density_token(&self) -> &'static str {
        RowDensity::ALL[self.density].config_token()
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
    pub fn language_for_token(token: Option<&str>) -> Option<Language> {
        // Map through the parallel token table (single source with the form
        // indices) so an unknown spelling can never resolve to a language.
        let index = Self::language_index_for(token);
        token.map(|_| match index {
            1 => Language::Zh,
            _ => Language::En,
        })
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

/// Render the settings overlay from the committed focus plan.  The focused
/// field is the plan's `SettingsField` control; any other control paints no
/// focus marker and scrolls to the top (fail-closed).
pub(super) fn render_settings_overlay_at(
    frame: &mut Frame<'_>,
    app: &crate::TuiApp,
    theme: TuiTheme,
    focus: super::TuiFocusPlan,
    popup: Rect,
) {
    let form = &app.settings_form;
    let focused_field = focus.settings_field();
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            theme.glyph(IconId::Settings),
            t("chrome.settings")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let footer_content = settings_footer(
        form,
        focused_field,
        theme,
        inner,
        app.first_run.view().info.is_some(),
    );
    let [body, footer] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(footer_content.height),
    ])
    .areas(inner);

    // Scroll the focused field into view on short terminals (the body shows
    // roughly `body.height - 1` rows; a focused field past that window is
    // still reachable via Tab and becomes visible as the offset follows it).
    // The layout owns the offset so the pointer hit map and the painted frame
    // can never disagree about which rows carry which fields.
    let layout = settings_overlay_layout(app, popup, focused_field);
    let rows = settings_field_rows(app);
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(rows.len() + 2);
    for row in rows.iter().filter(|row| row.field < 30) {
        lines.push(field_line(
            focused_field,
            row.field,
            &row.label,
            &row.value,
            theme,
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        format!("── {} ──", t("settings.privileges")),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    )]));
    for row in rows.iter().filter(|row| row.field >= 30) {
        lines.push(field_line(
            focused_field,
            row.field,
            &row.label,
            &row.value,
            theme,
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((layout.scroll, 0)),
        body,
    );

    frame.render_widget(
        Paragraph::new(footer_content.lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center),
        footer,
    );
}

fn mode_display_label(index: usize) -> &'static str {
    match ModePreference::CHOICES
        .get(index)
        .copied()
        .unwrap_or_default()
    {
        ModePreference::Explicit(LightDark::Light) => t("settings.light"),
        ModePreference::Explicit(LightDark::EyeForest) => t("settings.eyeforest"),
        ModePreference::FollowNative => t("settings.system"),
        ModePreference::Explicit(LightDark::Dark) => t("settings.dark"),
    }
}

/// The painted label of one font row. The bundled segment shows the face it
/// persists (a family name is data, not copy), and the host segment is catalog
/// text shared with the desktop frontends.
fn font_display_label(role: FontRole, index: usize) -> &'static str {
    match FontChoice::PICKER_LADDER
        .get(index)
        .copied()
        .unwrap_or(FontChoice::Bundled)
    {
        FontChoice::System => t("settings.font_system"),
        FontChoice::Bundled => role.bundled_family(),
        FontChoice::Custom(family) => family,
    }
}

fn density_display_label(index: usize) -> &'static str {
    match RowDensity::ALL
        .get(index)
        .copied()
        .unwrap_or(RowDensity::Comfortable)
    {
        RowDensity::Compact => t("settings.density_compact"),
        RowDensity::Comfortable => t("settings.density_comfortable"),
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
    focused_field: Option<usize>,
    field: usize,
    label: &'a str,
    value: &'a str,
    theme: TuiTheme,
) -> Line<'static> {
    let focused = focused_field == Some(field);
    let marker = if focused { "▸ " } else { "  " };
    Line::from(vec![
        Span::styled(
            format!("{marker}{}", super::text::pad_cells(label, 16)),
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
            Style::new().fg(if focused {
                theme.color(Color::White)
            } else {
                theme.dim
            }),
        ),
    ])
}

#[cfg(test)]
#[path = "../../tests/gui/ui/settings_tests.rs"]
mod tests;
