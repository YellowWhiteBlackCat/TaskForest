//! Settings page: live preference rows read from — and applied through — the
//! existing shared authorities. No local copy of any preference exists: every
//! row projects its authority at mount time and every activation writes the
//! authority, then asks for the remount that re-renders the fresh values.
//!
//! **Authorities and their write entries** (all pre-existing, none invented):
//! - theme light/dark — this frontend's render authority is the
//!   `WindowPalette` resource (+ the camera clear color); a choice
//!   re-resolves it from the theme tokens and the remount restyles the page
//!   through the `TextRole` observer and the nav rail through the route
//!   observer. The persisted skin/mode/contrast restore runs in the startup
//!   composition (`restore_persisted_preferences`).
//! - language — the process-global i18n bundle (`i18n::current_language` /
//!   `i18n::set_language`), the same entry the TUI/GPUI language pills use.
//! - refresh cadence — [`ShellApp::telemetry_interval`] /
//!   [`ShellApp::set_telemetry_interval`]; the drain applies the cadence to
//!   the platform client every frame.
//! - history capacity — [`ShellApp::history`] reads, `ShellApp::
//!   set_history_capacity`] writes (the store clamps to 10..=600).
//! - telemetry pause — [`ShellApp::paused`] reads, the shared
//!   `AppAction::TogglePause` reducer writes.
//!
//! **Widgets.** Choice rows use the official `bevy_ui_widgets` primitives —
//! `RadioButton` for the discrete selects (theme/language/cadence/capacity)
//! and `Checkbox` for the pause boolean — with `Checked` as the state
//! marker. The widget package's pointer/focus observers ride the windowed
//! composition; the page's activation observer (`settings_choice_observer`)
//! resolves the `ValueChange` activations back to their typed choices and
//! applies them, which is the same seam headless tests exercise.
//!
//! [`ShellApp::telemetry_interval`]: taskmanager_shell::ShellApp::telemetry_interval
//! [`ShellApp::set_telemetry_interval`]: taskmanager_shell::ShellApp::set_telemetry_interval
//! [`ShellApp::history`]: taskmanager_shell::ShellApp::history
//! [`ShellApp::set_history_capacity`]: taskmanager_shell::ShellApp::set_history_capacity
//! [`ShellApp::paused`]: taskmanager_shell::ShellApp::paused

use std::time::Duration;

use bevy::camera::ClearColor;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut, SystemParam};
use bevy::scene::{EntityScene, Scene, bsn};
use bevy::ui::Checked;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, FlexDirection, Node, Overflow, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Checkbox, RadioButton, RadioGroup, ScrollArea, ValueChange};
use taskmanager_application::i18n::{Language, current_language, set_language, t};
use taskmanager_application::{AppAction, ApplicationHistoryStatus, TelemetryInterval};
use taskmanager_core::config::Config;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use crate::app::{FrontendTrack, PageContext, RouteChanged, SharedRuntimeHandle};
use crate::pages::alerts::{page_observer, request_projection_refresh};
use crate::palette::{UiPalette, space_8, ui_palette};
use crate::window::{Role, TextRole, WindowPalette};
use taskmanager_application::ConfigSubmissionStatus;
use taskmanager_application::ConfigSubmitError;
use taskmanager_application::DEFAULT_CONFIG_INITIAL_WAIT;
use taskmanager_core::core::appearance::DesktopAppearance;
use taskmanager_core::core::appearance::PreferredColorScheme;

mod choices;
use choices::{capacity_entries, language_entries, refresh_entries, theme_entries};

mod privilege_center;
use privilege_center::privileges_section_scene;

#[derive(Component, Clone, Default)]
pub(crate) struct SettingsHeading;
#[derive(Component, Clone, Default)]
pub(crate) struct SettingsBody;
#[derive(Component, Clone, Default)]
pub(crate) struct SettingsFooter;

/// The telemetry refresh-cadence choices (ms), in display order — the same
/// four steps the TUI settings form exposes.
pub(crate) const REFRESH_CHOICES_MS: [u64; 4] = [500, 1000, 2000, 5000];

/// The history-capacity choices (samples), in display order — steps inside
/// the shared store's 10..=600 clamp (the same ladder as the graph-points
/// preference).
pub(crate) const CAPACITY_CHOICES: [usize; 4] = [60, 120, 300, 600];

/// The index of the live interval among the cadence choices; `None` when the
/// effective cadence is not one of the offered steps (a clamped or
/// externally-set interval) — rendered honestly as no selection.
pub(crate) fn refresh_choice_index(interval: TelemetryInterval) -> Option<usize> {
    let millis = u64::try_from(interval.duration().as_millis()).unwrap_or(u64::MAX);
    REFRESH_CHOICES_MS
        .iter()
        .position(|&choice| choice == millis)
}

/// The index of the live capacity among the capacity choices; `None` when
/// the effective capacity is not one of the offered steps.
pub(crate) fn capacity_choice_index(capacity: usize) -> Option<usize> {
    CAPACITY_CHOICES
        .iter()
        .position(|&choice| choice == capacity)
}

/// One of the two switchable theme modes resolved from the same token
/// combination as the cold start (GNOME skin, no high contrast, system
/// fonts) — a switch changes exactly one variable.
pub(crate) fn theme_for_mode(mode: LightDark) -> Theme {
    theme_for_mode_and_contrast(mode, HighContrast::Off)
}

pub(crate) fn theme_for_mode_and_contrast(mode: LightDark, hc: HighContrast) -> Theme {
    Theme::build(
        Skin::Gnome,
        mode,
        hc,
        ResolvedFonts::system_for(Skin::Gnome),
    )
}

/// Dynamic presentation theme preferences for the Bevy desktop shell.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ThemePreferences {
    /// Current measured zeros may use the muted resource-cell color.
    pub(crate) gray_zero_values: bool,
    /// Explicit mode choice, or `None` to follow the system desktop preference.
    pub(crate) mode: Option<LightDark>,
    /// Explicit skin choice, or `None` to follow default GNOME skin.
    pub(crate) skin: Option<Skin>,
    pub(crate) hc: bool,
    pub(crate) observed_appearance: Option<DesktopAppearance>,
}

impl ThemePreferences {
    #[must_use]
    pub(crate) fn effective_skin(&self) -> Skin {
        self.skin.unwrap_or(Skin::Gnome)
    }

    #[must_use]
    pub(crate) fn effective_mode(&self) -> LightDark {
        self.mode
            .unwrap_or_else(|| match self.observed_appearance.map(|a| a.color_scheme) {
                Some(PreferredColorScheme::Light) => LightDark::Light,
                _ => LightDark::Dark,
            })
    }

    #[must_use]
    pub(crate) fn effective_contrast(&self) -> HighContrast {
        let is_hc = self.hc
            || self
                .observed_appearance
                .and_then(|a| a.high_contrast)
                .unwrap_or(false);
        if is_hc {
            HighContrast::On
        } else {
            HighContrast::Off
        }
    }

    #[must_use]
    pub(crate) fn resolve_theme(&self) -> Theme {
        Theme::build(
            self.effective_skin(),
            self.effective_mode(),
            self.effective_contrast(),
            ResolvedFonts::system_for(self.effective_skin()),
        )
    }
}

/// Which of the two switchable modes the live palette currently reflects,
/// matched on the view-surface token. `None` when the palette came from any
/// other combination (a future skin picker, a high-contrast variant) —
/// rendered as no fabricated selection.
pub(crate) fn palette_mode(palette: &UiPalette) -> Option<LightDark> {
    // The palette carries the theme's mode identity, so the read-back is
    // exact for every skin. A mode the switch does not own (EyeForest, from
    // another frontend's config) is never claimed: the row renders no
    // fabricated selection.
    matches!(palette.mode, LightDark::Light | LightDark::Dark).then_some(palette.mode)
}

/// One discrete choice on the page: display label, the typed choice it owns,
/// and whether the live authority currently selects it.
struct ChoiceEntry {
    label: String,
    choice: SettingsChoice,
    selected: bool,
}

/// Identity of one interactive settings widget, carried by the widget entity
/// so the activation observer can resolve a `bevy_ui_widgets` `ValueChange`
/// back to the preference it owns. The struct wrapper + enum payload is the
/// bsn! template shape (one tuple field patched with a value, like the nav
/// rail's `NavTarget`); the `Default` seed exists for the template mechanism.
#[derive(Clone, Default, Component)]
pub(crate) struct SettingsChoice(pub(crate) SettingsField);

/// The typed preference one widget owns.
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) enum SettingsField {
    Theme(LightDark),
    SystemMode,
    HighContrast(bool),
    Language(Language),
    Refresh(TelemetryInterval),
    HistoryCapacity(usize),
    HistoryPersistence(bool),
    GrayZeroValues(bool),
    #[default]
    PauseTelemetry,
}

/// The activation applier: resolve the widget's typed choice, write it
/// through its authority, then request the remount that shows the fresh
/// values ("changes take effect immediately; no local copy state").
/// Submit a patch to the shared [`ConfigCoordinator`] via [`SharedRuntime::lock_config`].
pub(crate) fn patch_persisted_config<F>(
    runtime: Option<&SharedRuntimeHandle>,
    patch: F,
) -> Option<Result<ConfigSubmissionStatus, ConfigSubmitError>>
where
    F: FnOnce(&mut Config),
{
    let runtime = runtime?;
    let mut config_guard = runtime.shared.lock_config();
    let client = config_guard.as_mut()?;

    let _ = client.drain();

    let current = client.snapshot().cloned()?;
    let mut updated = (*current).clone();
    patch(&mut updated);

    Some(client.try_submit(updated))
}

// ---- persisted-preference restore (composition wiring) ----------------------

/// Apply one persisted config snapshot to the live authorities: the theme
/// preferences (mode/skin/contrast) re-resolve the window palette and clear
/// color, the locale reaches the process-global i18n bundle, and the saved
/// telemetry cadence and history capacity reach the shared shell. An absent
/// preferences resource (headless compositions without the settings plugin)
/// restores only the shell-facing state.
pub(crate) fn apply_persisted_config(
    config: &Config,
    prefs: Option<&mut ThemePreferences>,
    palette: &mut WindowPalette,
    clear: Option<&mut ClearColor>,
    track: &mut FrontendTrack,
) {
    if let Some(prefs) = prefs {
        prefs.gray_zero_values = config.gray_zero_values;
        if config.mode.eq_ignore_ascii_case("System") || config.mode.is_empty() {
            prefs.mode = None;
        } else if config.mode.eq_ignore_ascii_case("Light") {
            prefs.mode = Some(LightDark::Light);
        } else if config.mode.eq_ignore_ascii_case("Dark") {
            prefs.mode = Some(LightDark::Dark);
        } else if config.mode.eq_ignore_ascii_case("EyeForest") {
            prefs.mode = Some(LightDark::EyeForest);
        }

        if !config.skin.is_empty() {
            prefs.skin = match config.skin.to_ascii_lowercase().as_str() {
                "kde" => Some(Skin::Kde),
                "windows" => Some(Skin::Windows),
                "macos" => Some(Skin::Macos),
                _ => Some(Skin::Gnome),
            };
        }

        prefs.hc = config.hc;
        apply_preferences(prefs, palette, clear);
    }

    if let Some(lang) = config.language.as_deref().and_then(Language::from_code) {
        set_language(lang);
    }

    if config.refresh_ms > 0 {
        track
            .shell
            .set_telemetry_interval(TelemetryInterval::clamped(Duration::from_millis(
                config.refresh_ms,
            )));
    }

    if config.graph_data_points > 0 {
        track
            .shell
            .set_history_capacity(usize::try_from(config.graph_data_points).unwrap_or(60));
    }
}

/// Synchronize preferences from the coordinator via [`SharedRuntimeHandle`]:
/// the bounded initial wait (first call) or an in-memory drain (afterwards),
/// then one snapshot application. Absent runtime or client resolves nothing.
pub(crate) fn sync_preferences_from_config(
    runtime: Option<&SharedRuntimeHandle>,
    prefs: Option<&mut ThemePreferences>,
    palette: &mut WindowPalette,
    clear: Option<&mut ClearColor>,
    track: &mut FrontendTrack,
) {
    let Some(runtime) = runtime else {
        return;
    };
    let mut config_guard = runtime.shared.lock_config();
    let Some(client) = config_guard.as_mut() else {
        return;
    };

    if client.snapshot().is_none() {
        let _ = client.wait_for_initial(DEFAULT_CONFIG_INITIAL_WAIT);
    } else {
        let _ = client.drain();
    }

    if let Some(snapshot) = client.snapshot().cloned() {
        apply_persisted_config(&snapshot, prefs, palette, clear, track);
    }
}

/// The one-shot composition restore: the persisted appearance, locale, and
/// telemetry cadence/capacity reach their authorities before the first frame
/// renders. A cold-start-unknown or absent client keeps the defaults —
/// unavailable state is never fabricated.
pub(crate) fn restore_persisted_preferences(
    runtime: Option<Res<SharedRuntimeHandle>>,
    mut prefs: Option<ResMut<ThemePreferences>>,
    mut palette: ResMut<WindowPalette>,
    mut clear: Option<ResMut<ClearColor>>,
    mut track: NonSendMut<FrontendTrack>,
) {
    sync_preferences_from_config(
        runtime.as_deref(),
        prefs.as_deref_mut(),
        &mut palette,
        clear.as_deref_mut(),
        &mut track,
    );
}

/// The appearance authorities a settings choice writes through: the optional
/// persisted theme preferences, the resolved window palette every renderer
/// reads, and the optional camera clear color (absent headless).
#[derive(SystemParam)]
struct SettingsAppearanceTargets<'w> {
    /// Persisted theme preferences, when the settings page installed them.
    prefs: Option<ResMut<'w, ThemePreferences>>,
    /// The resolved window palette.
    palette: ResMut<'w, WindowPalette>,
    /// The camera clear color (absent headless).
    clear: Option<ResMut<'w, ClearColor>>,
}

fn settings_choice_observer(
    change: On<ValueChange<bool>>,
    choices: Query<&SettingsChoice>,
    mut track: NonSendMut<FrontendTrack>,
    mut appearance: SettingsAppearanceTargets,
    runtime: Option<Res<SharedRuntimeHandle>>,
    mut commands: Commands,
) {
    let Ok(choice) = choices.get(change.event().source) else {
        return; // a foreign widget: none of this page's business
    };
    match choice.0.clone() {
        SettingsField::Theme(mode) => {
            if let Some(prefs) = appearance.prefs.as_deref_mut() {
                prefs.mode = Some(mode);
                apply_preferences(
                    prefs,
                    &mut appearance.palette,
                    appearance.clear.as_deref_mut(),
                );
            } else {
                apply_theme(
                    mode,
                    &mut appearance.palette,
                    appearance.clear.as_deref_mut(),
                );
            }
            let _ = patch_persisted_config(runtime.as_deref(), |cfg| {
                cfg.mode = match mode {
                    LightDark::Light => "Light".to_string(),
                    LightDark::Dark => "Dark".to_string(),
                    LightDark::EyeForest => "EyeForest".to_string(),
                };
            });
        }
        SettingsField::SystemMode => {
            if let Some(prefs) = appearance.prefs.as_deref_mut() {
                prefs.mode = None;
                apply_preferences(
                    prefs,
                    &mut appearance.palette,
                    appearance.clear.as_deref_mut(),
                );
            }
            let _ = patch_persisted_config(runtime.as_deref(), |cfg| {
                cfg.mode = "System".to_string();
            });
        }
        SettingsField::HighContrast(on) => {
            if let Some(prefs) = appearance.prefs.as_deref_mut() {
                prefs.hc = on;
                apply_preferences(
                    prefs,
                    &mut appearance.palette,
                    appearance.clear.as_deref_mut(),
                );
            } else {
                appearance.palette.inner.high_contrast = on;
            }
            let _ = patch_persisted_config(runtime.as_deref(), |cfg| {
                cfg.hc = on;
            });
        }
        SettingsField::Language(language) => {
            set_language(language);
            let _ = patch_persisted_config(runtime.as_deref(), |cfg| {
                cfg.language = Some(language.code().to_string());
            });
        }
        SettingsField::Refresh(interval) => {
            track.shell.set_telemetry_interval(interval);
            let _ = patch_persisted_config(runtime.as_deref(), |cfg| {
                cfg.refresh_ms = u64::try_from(interval.duration().as_millis()).unwrap_or(u64::MAX);
            });
        }
        SettingsField::HistoryCapacity(capacity) => {
            track.shell.set_history_capacity(capacity);
            let _ = patch_persisted_config(runtime.as_deref(), |cfg| {
                cfg.graph_data_points = u32::try_from(capacity).unwrap_or(u32::MAX);
            });
        }
        SettingsField::HistoryPersistence(enabled) => {
            let submitted = patch_persisted_config(runtime.as_deref(), |cfg| {
                cfg.history_persistence = enabled;
            });
            if let Err(error) = submitted.unwrap_or(Err(ConfigSubmitError::NotReady)) {
                track.shell.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::TIMED_LONG,
                    t("settings.config_not_queued").replace("{error}", &error.to_string()),
                );
            }
        }
        SettingsField::GrayZeroValues(enabled) => {
            if let Some(prefs) = appearance.prefs.as_deref_mut() {
                prefs.gray_zero_values = enabled;
            }
            let _ = patch_persisted_config(runtime.as_deref(), |cfg| {
                cfg.gray_zero_values = enabled;
            });
        }
        SettingsField::PauseTelemetry => {
            // Guarded so a repeated activation is a no-op, not a double flip.
            if track.shell.paused() != change.event().value {
                let _ = track.shell.apply_action(AppAction::TogglePause);
            }
        }
    }
    commands.trigger(RouteChanged);
}

/// Apply resolved theme preferences to the palette resource and clear color.
pub(crate) fn apply_preferences(
    prefs: &ThemePreferences,
    palette: &mut WindowPalette,
    clear: Option<&mut ClearColor>,
) {
    let theme = prefs.resolve_theme();
    palette.inner = ui_palette(&theme);
    if let Some(clear) = clear {
        clear.0 = palette.inner.window_clear;
    }
}

/// Re-resolve the render authorities from the theme tokens for `mode`. The
/// camera clear color is absent in the headless composition and stays
/// untouched there.
fn apply_theme(mode: LightDark, palette: &mut WindowPalette, clear: Option<&mut ClearColor>) {
    palette.inner = ui_palette(&theme_for_mode(mode));
    if let Some(clear) = clear {
        clear.0 = palette.inner.window_clear;
    }
}

// ---- view model rows (pure projections of the authorities) ----

// ---- render adapters ----

/// Content-region scene for the Settings page.
pub(crate) fn content(context: &PageContext<'_>) -> impl Scene + use<> {
    let interval = context.shell.telemetry_interval();
    let capacity = context.shell.history.capacity();
    let paused = context.shell.paused();
    let language = current_language();
    let mode = palette_mode(context.palette);
    let hc = context.palette.high_contrast;
    let persistence = context.history.status != ApplicationHistoryStatus::Disabled;
    let rows: Vec<Box<dyn Scene>> = vec![
        toggle_row(
            t("settings.gray_zero_values"),
            ChoiceEntry {
                label: t("settings.gray_zero_values").to_owned(),
                choice: SettingsChoice(SettingsField::GrayZeroValues(!context.gray_zero_values)),
                selected: context.gray_zero_values,
            },
            t("settings.gray_zero_values_hint"),
        ),
        radio_row(
            "Theme",
            theme_entries(mode),
            match mode {
                Some(LightDark::Light) => "Light",
                Some(LightDark::Dark) => "Dark",
                _ => "System",
            },
        ),
        toggle_row(
            "High contrast",
            ChoiceEntry {
                label: "enabled".to_owned(),
                choice: SettingsChoice(SettingsField::HighContrast(!hc)),
                selected: hc,
            },
            if hc { "enabled" } else { "disabled" },
        ),
        radio_row(
            "Language",
            language_entries(language),
            match language {
                Language::En => "en",
                Language::Zh => "zh",
            },
        ),
        radio_row(
            "Refresh interval",
            refresh_entries(interval),
            &format!(
                "{} ms",
                u64::try_from(interval.duration().as_millis()).unwrap_or(u64::MAX)
            ),
        ),
        radio_row(
            "History capacity",
            capacity_entries(capacity),
            &format!("{capacity} samples"),
        ),
        toggle_row(
            t("settings.history_persistence"),
            ChoiceEntry {
                label: t("settings.history_persistence").to_owned(),
                choice: SettingsChoice(SettingsField::HistoryPersistence(!persistence)),
                selected: persistence,
            },
            if persistence { "enabled" } else { "disabled" },
        ),
        toggle_row(
            "Telemetry updates",
            ChoiceEntry {
                label: "paused".to_owned(),
                choice: SettingsChoice(SettingsField::PauseTelemetry),
                selected: paused,
            },
            if paused { "paused" } else { "live" },
        ),
    ];
    let mut rows = rows;
    rows.insert(0, privileges_section_scene(context.shell, context.palette));
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_8())),
        }
        BackgroundColor({ context.palette.content_bg })
        Children [
            Node { min_height: px(context.palette.control_height_px + space_8()), flex_shrink: 0.0 }
            SettingsHeading
            Children [ Text({ crate::app::Page::Settings.title() }) TextRole(Role::Heading) ] --
            Node {
                width: percent(100), flex_grow: 1.0, flex_basis: px(0.0), min_height: px(0.0),
                flex_direction: FlexDirection::Column, row_gap: Val::Px(space_8()),
                overflow: Overflow::scroll_y(),
            }
            ScrollArea
            SettingsBody
            Children [ { rows } ] --

                Node { min_height: px(context.palette.control_height_px + space_8()), flex_shrink: 0.0 }
                SettingsFooter
                Children [
                    Text("Choices apply live through the shared shell seams and persist across sessions through the shared config coordinator")
                    TextRole(Role::Caption)
                ]
            --
            { EntityScene(page_observer(request_projection_refresh)) }--
            { EntityScene(page_observer(settings_choice_observer)) }
        ]
    }
}

/// A select-style row: caption label, the radio group, and the live value.
fn radio_row(label: &str, entries: Vec<ChoiceEntry>, value: &str) -> Box<dyn Scene> {
    let label = label.to_owned();
    let value = value.to_owned();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
             Node { width: px(160.0), height: Val::Auto } Children [
                 Text(label) TextRole(Role::Caption)
            ] --

                Node {
                    height: Val::Auto,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                RadioGroup
                Children [ { choice_widgets(entries) } ]
            --
             Text(value) TextRole(Role::Caption)
        ]
    })
}

/// A boolean row: caption label, one checkbox, and the live value.
fn toggle_row(label: &str, entry: ChoiceEntry, value: &str) -> Box<dyn Scene> {
    let label = label.to_owned();
    let value = value.to_owned();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
             Node { width: px(160.0), height: Val::Auto } Children [
                 Text(label) TextRole(Role::Caption)
            ] --
            { choice_widgets(vec![entry]) }--
             Text(value) TextRole(Role::Caption)
        ]
    })
}

/// One widget per choice. The shape pairs differ by the widget primitive
/// (`Checkbox` for the boolean pause toggle, `RadioButton` for the discrete
/// selects) and by the `Checked` marker, so the fan-out boxes the scenes
/// (the documented dynamic-children seam). The primitives are the official
/// unstyled ones; state is the `Checked` marker.
fn choice_widgets(entries: Vec<ChoiceEntry>) -> Vec<Box<dyn Scene>> {
    entries
        .into_iter()
        .map(|entry| {
            let ChoiceEntry {
                label,
                choice,
                selected,
            } = entry;
            let SettingsChoice(field) = choice;
            let boolean = matches!(
                field,
                SettingsField::PauseTelemetry
                    | SettingsField::HistoryPersistence(_)
                    | SettingsField::GrayZeroValues(_)
            );
            match (boolean, selected) {
                (true, true) => Box::new(checked_checkbox_shape(label, field)) as Box<dyn Scene>,
                (true, false) => Box::new(unchecked_checkbox_shape(label, field)),
                (false, true) => Box::new(checked_radio_shape(label, field)),
                (false, false) => Box::new(unchecked_radio_shape(label, field)),
            }
        })
        .collect()
}

fn checked_radio_shape(label: String, field: SettingsField) -> impl Scene + use<> {
    bsn! {
        Node {
            height: px(28.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        RadioButton
        Checked
        SettingsChoice(field)
        Children [
             Text(label) TextRole(Role::Body)
        ]
    }
}

fn unchecked_radio_shape(label: String, field: SettingsField) -> impl Scene + use<> {
    bsn! {
        Node {
            height: px(28.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        RadioButton
        SettingsChoice(field)
        Children [
             Text(label) TextRole(Role::Body)
        ]
    }
}

fn checked_checkbox_shape(label: String, field: SettingsField) -> impl Scene + use<> {
    bsn! {
        Node {
            height: px(28.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        Checkbox
        Checked
        SettingsChoice(field)
        Children [
             Text(label) TextRole(Role::Body)
        ]
    }
}

fn unchecked_checkbox_shape(label: String, field: SettingsField) -> impl Scene + use<> {
    bsn! {
        Node {
            height: px(28.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        Checkbox
        SettingsChoice(field)
        Children [
             Text(label) TextRole(Role::Body)
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/settings.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/config_sync.rs"]
mod config_sync;
