//! Settings page: live preference rows read from — and applied through — the
//! existing shared authorities. No local copy of any preference exists: every
//! row projects its authority at mount time and every activation writes the
//! authority, then asks for the remount that re-renders the fresh values.
//!
//! **Authorities and their write entries** (all pre-existing, none invented):
//! - theme light/dark — this frontend's render authority is the
//!   [`WindowPalette`] resource (+ the camera clear color); a choice
//!   re-resolves it from the theme tokens and the remount restyles the page
//!   through the `TextRole` observer and the nav rail through the route
//!   observer. Full-chrome text retheming (header/summary ink stamped once
//!   at startup) lands with the persisted skin/mode restoration milestone.
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
//! composition; the page's activation observer ([`settings_choice_observer`])
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
use bevy::ecs::system::{Commands, NonSendMut, Query, ResMut};
use bevy::scene::{EntityScene, Scene, bsn};
use bevy::ui::Checked;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, FlexDirection, Node, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Checkbox, RadioButton, RadioGroup, ValueChange};
use taskmanager_application::i18n::{Language, current_language, set_language};
use taskmanager_application::{AppAction, TelemetryInterval};

use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use crate::app::{FrontendTrack, PageContext, RouteChanged};
use crate::pages::alerts::{page_observer, request_projection_refresh};
use crate::palette::{UiPalette, space_8, ui_palette};
use crate::window::{Role, TextRole, WindowPalette};

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
    Theme::build(
        Skin::Gnome,
        mode,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
}

/// Which of the two switchable modes the live palette currently reflects,
/// matched on the view-surface token. `None` when the palette came from any
/// other combination (a future skin picker, a high-contrast variant) —
/// rendered as no fabricated selection.
pub(crate) fn palette_mode(palette: &UiPalette) -> Option<LightDark> {
    [LightDark::Light, LightDark::Dark]
        .into_iter()
        .find(|&mode| ui_palette(&theme_for_mode(mode)).content_bg == palette.content_bg)
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
    Language(Language),
    Refresh(TelemetryInterval),
    HistoryCapacity(usize),
    #[default]
    PauseTelemetry,
}

/// The activation applier: resolve the widget's typed choice, write it
/// through its authority, then request the remount that shows the fresh
/// values ("changes take effect immediately; no local copy state").
fn settings_choice_observer(
    change: On<ValueChange<bool>>,
    choices: Query<&SettingsChoice>,
    mut track: NonSendMut<FrontendTrack>,
    mut palette: ResMut<WindowPalette>,
    mut clear: Option<ResMut<ClearColor>>,
    mut commands: Commands,
) {
    let Ok(choice) = choices.get(change.event().source) else {
        return; // a foreign widget: none of this page's business
    };
    match choice.0.clone() {
        SettingsField::Theme(mode) => apply_theme(mode, &mut palette, clear.as_deref_mut()),
        SettingsField::Language(language) => set_language(language),
        SettingsField::Refresh(interval) => track.shell.set_telemetry_interval(interval),
        SettingsField::HistoryCapacity(capacity) => track.shell.set_history_capacity(capacity),
        SettingsField::PauseTelemetry => {
            // Guarded so a repeated activation is a no-op, not a double flip.
            if track.shell.paused() != change.event().value {
                let _ = track.shell.apply_action(AppAction::TogglePause);
            }
        }
    }
    commands.trigger(RouteChanged);
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

fn theme_entries(mode: Option<LightDark>) -> Vec<ChoiceEntry> {
    [(LightDark::Light, "Light"), (LightDark::Dark, "Dark")]
        .into_iter()
        .map(|(value, label)| ChoiceEntry {
            label: label.to_owned(),
            choice: SettingsChoice(SettingsField::Theme(value)),
            selected: mode == Some(value),
        })
        .collect()
}

fn language_entries(language: Language) -> Vec<ChoiceEntry> {
    [(Language::En, "English"), (Language::Zh, "中文")]
        .into_iter()
        .map(|(value, label)| ChoiceEntry {
            label: label.to_owned(),
            choice: SettingsChoice(SettingsField::Language(value)),
            selected: language == value,
        })
        .collect()
}

fn refresh_entries(interval: TelemetryInterval) -> Vec<ChoiceEntry> {
    let selected = refresh_choice_index(interval);
    REFRESH_CHOICES_MS
        .iter()
        .enumerate()
        .map(|(index, millis)| ChoiceEntry {
            label: refresh_label(*millis),
            choice: SettingsChoice(SettingsField::Refresh(interval_for_millis(*millis))),
            selected: selected == Some(index),
        })
        .collect()
}

fn capacity_entries(capacity: usize) -> Vec<ChoiceEntry> {
    let selected = capacity_choice_index(capacity);
    CAPACITY_CHOICES
        .iter()
        .enumerate()
        .map(|(index, samples)| ChoiceEntry {
            label: samples.to_string(),
            choice: SettingsChoice(SettingsField::HistoryCapacity(*samples)),
            selected: selected == Some(index),
        })
        .collect()
}

/// The `TelemetryInterval` for one offered cadence step. The ladder lives
/// inside the policy's clamp window, so `clamped` never deviates from the
/// requested step.
fn interval_for_millis(millis: u64) -> TelemetryInterval {
    TelemetryInterval::clamped(Duration::from_millis(millis))
}

fn refresh_label(millis: u64) -> String {
    format!(
        "{} s",
        f64::from(u32::try_from(millis).unwrap_or(u32::MAX)) / 1000.0
    )
}

// ---- render adapters ----

/// Content-region scene for the Settings page.
pub(crate) fn content(context: &PageContext<'_>) -> impl Scene + use<> {
    let interval = context.shell.telemetry_interval();
    let capacity = context.shell.history.capacity();
    let paused = context.shell.paused();
    let language = current_language();
    let mode = palette_mode(context.palette);
    let rows: Vec<Box<dyn Scene>> = vec![
        radio_row(
            "Theme",
            theme_entries(mode),
            match mode {
                Some(LightDark::Light) => "Light",
                Some(_) => "Dark",
                None => "custom skin",
            },
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
            "Telemetry updates",
            ChoiceEntry {
                label: "paused".to_owned(),
                choice: SettingsChoice(SettingsField::PauseTelemetry),
                selected: paused,
            },
            if paused { "paused" } else { "live" },
        ),
    ];
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
            ( Text({ crate::app::Page::Settings.title() }) TextRole(Role::Heading) ),
            { rows },
            (
                Text("Choices apply live through the shared shell seams; cross-session persistence is incubating until the config write-back seam lands")
                TextRole(Role::Caption)
            ),
            { EntityScene(page_observer(request_projection_refresh)) },
            { EntityScene(page_observer(settings_choice_observer)) },
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
            ( Node { width: px(160.0), height: Val::Auto } Children [
                ( Text(label) TextRole(Role::Caption) ),
            ] ),
            (
                Node {
                    height: Val::Auto,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                RadioGroup
                Children [ { choice_widgets(entries) } ]
            ),
            ( Text(value) TextRole(Role::Caption) ),
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
            ( Node { width: px(160.0), height: Val::Auto } Children [
                ( Text(label) TextRole(Role::Caption) ),
            ] ),
            { choice_widgets(vec![entry]) },
            ( Text(value) TextRole(Role::Caption) ),
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
            let boolean = matches!(field, SettingsField::PauseTelemetry);
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
            ( Text(label) TextRole(Role::Body) ),
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
            ( Text(label) TextRole(Role::Body) ),
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
            ( Text(label) TextRole(Role::Body) ),
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
            ( Text(label) TextRole(Role::Body) ),
        ]
    }
}

#[cfg(test)]
#[path = "../../tests/headless/pages/settings.rs"]
mod tests;
