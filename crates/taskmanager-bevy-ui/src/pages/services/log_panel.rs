//! Service log panel: the bevy surface for the shell's renderer-neutral
//! service-log lifecycle (ADR-027, `ShellApp::open_service_log*`).
//!
//! Ownership line: the shell owns the open stream, the bounded feed, the
//! filters, the throttle and the fold path; the platform worker owns the
//! provider; this module owns ONLY the product surface — the open affordance
//! on the Services page, the panel scene, the panel-local key chords
//! (frontend-local, TUI `ServiceLogPanel` parity), and the fingerprint-gated
//! repaint. It never parses log text and never touches the platform client:
//! effects ride [`crate::input::PendingEffects`] like every other seam.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::ecs::world::World;
use bevy::input::keyboard::KeyCode;
use bevy::scene::{CommandsSceneExt, Scene, bsn, on, template_value};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, Display, FlexDirection, JustifyContent, Node,
    Overflow, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use taskmanager_application::i18n::t;
use taskmanager_core::core::services::{
    ServiceLogAvailability, ServiceLogEntry, ServiceLogErrorKind, ServiceLogLevelFilter,
    ServiceLogProviderState, ServiceLogTimeFilter,
};
use taskmanager_shell::app::OpenServiceLog;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

use crate::app::FrontendTrack;
use crate::drain::{ShellProjectionFolded, unix_now_ms};
use crate::input::PendingEffects;
use crate::palette::{UiPalette, no_wrap_text, space_2, space_4, space_8, space_12};
use crate::widgets::controls::{ControlTone, ControlVisual};
use crate::window::{Role, TextRole, WindowPalette};

// ---- pure view model -------------------------------------------------------

/// One panel-local control, resolved from a bare key while the panel owns the
/// keyboard. Frontend-local vocabulary (TUI `ServiceLogPanel` parity): the
/// shell has no shared chord for these, so the seam consumes them ahead of
/// the shell routers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServiceLogControlAction {
    ToggleFollow,
    TogglePaused,
    CycleLevel,
    CycleTime,
    Export,
    Close,
}

/// Bare-key → panel action. `None` for everything the panel does not own —
/// an untouched key never consumes.
#[must_use]
pub(crate) fn log_panel_key(key: KeyCode) -> Option<ServiceLogControlAction> {
    match key {
        KeyCode::KeyF => Some(ServiceLogControlAction::ToggleFollow),
        KeyCode::KeyP => Some(ServiceLogControlAction::TogglePaused),
        KeyCode::KeyL => Some(ServiceLogControlAction::CycleLevel),
        KeyCode::KeyT => Some(ServiceLogControlAction::CycleTime),
        KeyCode::KeyE => Some(ServiceLogControlAction::Export),
        KeyCode::Escape => Some(ServiceLogControlAction::Close),
        _ => None,
    }
}

/// The repaint gate: every feed fact the panel renders, in one comparable
/// value. A fold that changes none of these produces no work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LogFingerprint {
    open: bool,
    entries: usize,
    last_cursor: Option<String>,
    follow: bool,
    paused: bool,
    level: ServiceLogLevelFilter,
    time: ServiceLogTimeFilter,
    availability: ServiceLogAvailability,
    failure: Option<(ServiceLogErrorKind, Option<String>)>,
    last_success_ms: Option<u64>,
}

const CLOSED_FINGERPRINT: fn() -> LogFingerprint = || LogFingerprint {
    open: false,
    entries: 0,
    last_cursor: None,
    follow: false,
    paused: false,
    level: ServiceLogLevelFilter::All,
    time: ServiceLogTimeFilter::All,
    availability: ServiceLogAvailability::Loading,
    failure: None,
    last_success_ms: None,
};

/// The fingerprint of whatever the shell's log lifecycle holds right now.
#[must_use]
pub(crate) fn log_fingerprint(open: Option<&OpenServiceLog>) -> LogFingerprint {
    let Some(open) = open else {
        return CLOSED_FINGERPRINT();
    };
    let feed = &open.feed;
    LogFingerprint {
        open: true,
        entries: feed.entries().len(),
        last_cursor: feed.last_cursor().map(str::to_owned),
        follow: feed.follow,
        paused: feed.paused,
        level: feed.level,
        time: feed.time,
        availability: feed.provider.availability,
        failure: feed
            .provider
            .failure
            .as_ref()
            .map(|failure| (failure.kind, failure.detail.clone())),
        last_success_ms: feed.provider.last_success_ms,
    }
}

/// Resource mirror of the fingerprint the panel last rendered. A fold whose
/// fingerprint matches repaints nothing.
#[derive(Resource, Default)]
pub(crate) struct ServicesLogRenderState {
    pub(crate) rendered: Option<LogFingerprint>,
}

/// Honest status caption from the typed provider state. Healthy states render
/// no caption — the entries speak for themselves.
#[must_use]
pub(crate) fn log_status_caption(provider: &ServiceLogProviderState) -> String {
    if let Some(failure) = &provider.failure {
        let base = match failure.kind {
            ServiceLogErrorKind::TimedOut => t("svc.logs_timeout"),
            ServiceLogErrorKind::PermissionDenied => t("svc.logs_permission_denied"),
            ServiceLogErrorKind::Unsupported => t("svc.logs_unsupported"),
            ServiceLogErrorKind::MissingTool
            | ServiceLogErrorKind::TemporarilyUnavailable
            | ServiceLogErrorKind::ProviderFailed => t("svc.logs_failed"),
        };
        return match failure.detail.as_deref().map(str::trim) {
            Some(detail) if !detail.is_empty() => format!("{base} · {detail}"),
            _ => base.to_owned(),
        };
    }
    match provider.availability {
        ServiceLogAvailability::Loading => t("svc.logs_loading").to_owned(),
        ServiceLogAvailability::Empty => t("svc.logs_empty").to_owned(),
        ServiceLogAvailability::Available
        | ServiceLogAvailability::CaughtUp
        | ServiceLogAvailability::Disconnected
        | ServiceLogAvailability::Stale
        | ServiceLogAvailability::Unavailable => String::new(),
    }
}

fn level_caption(level: ServiceLogLevelFilter) -> String {
    match level {
        ServiceLogLevelFilter::All => t("svc.logs_level_all").to_owned(),
        ServiceLogLevelFilter::Errors => t("svc.logs_level_errors").to_owned(),
        ServiceLogLevelFilter::WarningsAndErrors => t("svc.logs_level_warnings").to_owned(),
        ServiceLogLevelFilter::InfoAndAbove => t("svc.logs_level_info").to_owned(),
    }
}

fn time_caption(time: ServiceLogTimeFilter) -> String {
    match time {
        ServiceLogTimeFilter::All => t("svc.logs_time_all").to_owned(),
        ServiceLogTimeFilter::LastHour => t("svc.logs_time_hour").to_owned(),
        ServiceLogTimeFilter::LastDay => t("svc.logs_time_day").to_owned(),
    }
}

/// UTC wall clock from the provider's realtime stamp; entries without one
/// render an honest dash prefix — never a fabricated time.
fn entry_stamp(entry: &ServiceLogEntry) -> String {
    entry.realtime_timestamp_micros.map_or_else(
        || "--:--:--".to_owned(),
        |micros| {
            let secs = micros / 1_000_000;
            format!(
                "{:02}:{:02}:{:02}",
                (secs / 3600) % 24,
                (secs / 60) % 60,
                secs % 60
            )
        },
    )
}

// ---- scenes ----------------------------------------------------------------

fn entry_row_scene(entry: &ServiceLogEntry, _palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
            padding: UiRect::vertical(Val::Px(space_2())),
            overflow: Overflow::clip_x(),
        }
        Children [
            (
                Node {
                    width: px(64.0),
                    flex_shrink: 0.0,
                    overflow: Overflow::clip_x(),
                }
                Children [
                    (
                        Text(entry_stamp(entry))
                        TextRole(Role::Mono)
                        template_value(no_wrap_text())
                    )
                ]
            ),
            (
                Text({ entry.message.clone() })
                TextRole(Role::Mono)
                template_value(no_wrap_text())
            ),
        ]
    }
}

fn chip_button(
    action: ServiceLogControlAction,
    label: String,
    active: bool,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    Box::new(bsn! {
        (
            Node {
                height: px(palette.control_height_px),
                padding: UiRect::horizontal(Val::Px(space_12())),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
            }
            BackgroundColor({
                if active { palette.nav_active_bg } else { palette.content_bg }
            })
            ControlVisual(ControlTone::Surface, active)
            Button
            on(log_panel_control_activated)
            ServiceLogControlButton(action)
            Children [
                ( Text(label) TextRole(Role::Caption) template_value(no_wrap_text()) )
            ]
        )
    })
}

/// The panel scene: header (service word + close), the four control chips,
/// the honest status line, and the bounded filtered entries. Rendered fresh
/// by [`paint_log_panel`]; nothing here mutates the world.
pub(crate) fn service_log_panel_scene(shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let Some(open) = shell.service_log.as_ref() else {
        return Box::new(bsn! {
            Node { display: Display::None }
        });
    };
    let feed = &open.feed;
    let service = open
        .service_id()
        .map(|id| id.to_string())
        .unwrap_or_default();
    let title = format!("{} — {service}", t("svc.logs"));
    let visible = shell
        .visible_service_log_entries(unix_now_ms() * 1_000)
        .unwrap_or_default();
    let rows: Vec<Box<dyn Scene>> = visible
        .iter()
        .map(|entry| Box::new(entry_row_scene(entry, palette)) as Box<dyn Scene>)
        .collect();
    let status = log_status_caption(&feed.provider);
    let status_row: Vec<Box<dyn Scene>> = if status.is_empty() {
        Vec::new()
    } else {
        vec![Box::new(bsn! {
            Node { width: percent(100.0), overflow: Overflow::clip_x() }
            Children [
                (
                    Text(status)
                    ServicesLogStatusLine
                    TextRole(Role::Caption)
                    template_value(no_wrap_text())
                )
            ]
        }) as Box<dyn Scene>]
    };
    let list: Box<dyn Scene> = if rows.is_empty() {
        Box::new(bsn! {
            Node { width: percent(100.0) }
            Children [
                ( Text(t("svc.logs_empty")) TextRole(Role::Caption) )
            ]
        })
    } else {
        Box::new(bsn! {
            Node {
                width: percent(100.0),
                height: px(280.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
            }
            ScrollArea
            Children [
                { rows },
            ]
        })
    };
    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            padding: UiRect::all(Val::Px(space_12())),
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        ServicesLogPanelRoot
        Children [
            (
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                    (
                        Text(title)
                        TextRole(Role::Body)
                        template_value(no_wrap_text())
                    ),
                    ( Node { flex_grow: 1.0 } ),
                    ( { chip_button(ServiceLogControlAction::Close, t("common.close").to_owned(), false, palette) } ),
                ]
            ),
            (
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                Children [
                    ( { chip_button(ServiceLogControlAction::ToggleFollow, t("svc.logs_follow").to_owned(), feed.follow, palette) } ),
                    ( { chip_button(ServiceLogControlAction::TogglePaused, t("common.paused").to_owned(), feed.paused, palette) } ),
                    ( { chip_button(ServiceLogControlAction::CycleLevel, level_caption(feed.level), false, palette) } ),
                    ( { chip_button(ServiceLogControlAction::CycleTime, time_caption(feed.time), false, palette) } ),
                    ( { chip_button(ServiceLogControlAction::Export, t("common.export").to_owned(), false, palette) } ),
                ]
            ),
            (
                Node {
                    width: percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(space_2()),
                }
                LogEntriesBox
                Children [
                    { status_row },
                    ( { list } ),
                ]
            ),
        ]
    })
}

/// Marker on the page's "open logs" affordance.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ServicesLogsOpenButton;

/// Bevy 0.19 widget activation for the open affordance.
pub(crate) fn services_logs_button_activated(
    activate: On<Activate>,
    buttons: Query<&ServicesLogsOpenButton>,
    mut commands: Commands,
) {
    if buttons.get(activate.event().entity).is_ok() {
        commands.trigger(ServiceLogsRequested);
    }
}

/// The Services toolbar row: the open-logs affordance, right-aligned. A
/// disabled-looking (unselected) button still renders; the request observer
/// drops a selection-less request honestly.
#[allow(dead_code)]
pub(crate) fn logs_toolbar_scene(has_selection: bool, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
            ( Node { flex_grow: 1.0 } ),
            (
                Node {
                    height: px(palette.control_height_px),
                    padding: UiRect::horizontal(Val::Px(space_12())),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({
                    if has_selection { palette.nav_active_bg } else { palette.content_bg }
                })
                ControlVisual(ControlTone::Surface, has_selection)
                Button
                on(services_logs_button_activated)
                ServicesLogsOpenButton
                Children [
                    (
                        Text({ t("svc.logs").to_owned() })
                        TextRole(Role::Caption)
                        template_value(no_wrap_text())
                    )
                ]
            ),
        ]
    })
}

// ---- events, markers, observers -------------------------------------------

/// Marker on the panel root (one panel per Services page mount).
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ServicesLogPanelRoot;

/// Marker for one panel control button; the value is the typed action.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ServiceLogControlButton(pub(crate) ServiceLogControlAction);

impl Default for ServiceLogControlButton {
    /// bsn! template seed only — a spawned button always patches a real
    /// action.
    fn default() -> Self {
        Self(ServiceLogControlAction::Close)
    }
}

/// Marker for the panel's honest status caption.
#[derive(Component, Clone, Default)]
pub(crate) struct ServicesLogStatusLine;

/// Marker for the bounded entries list (tests + future scroll tail).
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct LogEntriesBox;

/// Marker on the page-root container that hosts the panel scene.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct ServicesLogPanelSlot;

/// The open affordance fired by the Services page button (and tests).
#[derive(Event)]
pub(crate) struct ServiceLogsRequested;

/// The single repaint trigger for the panel: open, controls, close, and
/// folds with a changed fingerprint all converge here.
#[derive(Event)]
pub(crate) struct LogPanelRepaintRequired;

/// Optional directory for service log exports; `None` defaults to current working directory.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct ServiceLogExportDir(pub(crate) Option<std::path::PathBuf>);

/// Export the currently visible service-log entries to `taskmanager-service-{safe_id}.log`.
pub(crate) fn export_service_log(shell: &mut ShellApp, export_dir: Option<&std::path::Path>) {
    let Some(open) = shell.service_log.as_ref() else {
        shell.report_notice(
            FeedbackSource::Persistence,
            FeedbackSeverity::Warning,
            FeedbackLifecycle::SHORT,
            t("svc.logs_nothing_to_export"),
        );
        return;
    };
    let Some(service_id) = open.service_id().map(ToString::to_string) else {
        shell.report_notice(
            FeedbackSource::Persistence,
            FeedbackSeverity::Warning,
            FeedbackLifecycle::SHORT,
            t("svc.logs_nothing_to_export"),
        );
        return;
    };
    let entries = shell
        .visible_service_log_entries(crate::drain::unix_now_ms() * 1_000)
        .unwrap_or_default()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    if entries.is_empty() {
        shell.report_notice(
            FeedbackSource::Persistence,
            FeedbackSeverity::Warning,
            FeedbackLifecycle::SHORT,
            t("svc.logs_nothing_to_export"),
        );
        return;
    }

    let safe_id: String = service_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let file_name = format!("taskmanager-service-{safe_id}.log");
    let destination = match export_dir {
        Some(dir) => dir.join(&file_name),
        None => std::path::PathBuf::from(&file_name),
    };

    let mut payload = entries
        .iter()
        .map(|entry| format!("[{:?}] {}", entry.level, entry.message))
        .collect::<Vec<_>>()
        .join("\n");
    if !payload.is_empty() {
        payload.push('\n');
    }

    match std::fs::write(&destination, payload) {
        Ok(()) => {
            shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Success,
                FeedbackLifecycle::SHORT,
                t("svc.logs_exported").replace("{path}", &destination.display().to_string()),
            );
        }
        Err(_) => {
            shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                t("svc.logs_export_failed"),
            );
        }
    }
}

/// Bevy 0.19 widget activation for the panel buttons: shell mutation first,
/// then the typed repaint trigger.
pub(crate) fn log_panel_control_activated(
    activate: On<Activate>,
    buttons: Query<&ServiceLogControlButton>,
    mut track: NonSendMut<FrontendTrack>,
    export_dir: Option<Res<ServiceLogExportDir>>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(activate.event().entity) else {
        return;
    };
    let shell = &mut track.shell;
    match button.0 {
        ServiceLogControlAction::ToggleFollow => shell.toggle_service_log_follow(),
        ServiceLogControlAction::TogglePaused => shell.toggle_service_log_paused(),
        ServiceLogControlAction::CycleLevel => shell.cycle_service_log_level(),
        ServiceLogControlAction::CycleTime => shell.cycle_service_log_time(),
        ServiceLogControlAction::Export => {
            let dir = export_dir.as_deref().and_then(|d| d.0.as_deref());
            export_service_log(shell, dir);
        }
        ServiceLogControlAction::Close => shell.close_service_log(),
    }
    commands.trigger(LogPanelRepaintRequired);
}

/// Open the stream for the page's selected service. No selection → the
/// request is dropped honestly (the button is disabled in that state).
pub(crate) fn on_services_logs_requested(
    _request: On<ServiceLogsRequested>,
    mut track: NonSendMut<FrontendTrack>,
    selection: Option<Res<super::ServiceSelection>>,
    mut pending: ResMut<PendingEffects>,
    mut commands: Commands,
) {
    let Some(target) = selection.as_ref().and_then(|state| state.target.as_ref()) else {
        return;
    };
    if let Some(effect) = track.shell.open_service_log_for(target.clone()) {
        pending.0.push(effect);
    }
    commands.trigger(LogPanelRepaintRequired);
}

/// Fold tail: a changed log fingerprint repaints only the panel; the body
/// painter keeps its own services-revision gate.
pub(crate) fn on_services_fold_log_gate(
    _fold: On<ShellProjectionFolded>,
    track: crate::app::ShellTrack,
    rendered: Res<ServicesLogRenderState>,
    mut commands: Commands,
) {
    let fingerprint = track
        .shell()
        .service_log
        .as_ref()
        .map(|open| log_fingerprint(Some(open)));
    if rendered.rendered != fingerprint {
        commands.trigger(LogPanelRepaintRequired);
    }
}

/// Repaint trigger → world painter: the queue bridge every repaint path
/// converges on (open, controls, close, fold fingerprint changes).
pub(crate) fn on_log_panel_repaint_required(
    _repaint: On<LogPanelRepaintRequired>,
    mut commands: Commands,
) {
    commands.queue(paint_log_panel);
}

/// Mount-time first paint: the slot spawns after the page's insert hook has
/// bound the observers, so a pre-seeded lifecycle (capture fixture, route-back
/// remount) still renders its panel without waiting for a fold.
pub(crate) fn on_log_panel_slot_added(
    _added: On<bevy::ecs::lifecycle::Add, ServicesLogPanelSlot>,
    mut commands: Commands,
) {
    commands.queue(paint_log_panel);
}

/// The one panel painter (world form, queued like `paint_services`).
pub(crate) fn paint_log_panel(world: &mut World) {
    let palette = world.resource::<WindowPalette>().inner.clone();
    let fingerprint = world
        .non_send::<FrontendTrack>()
        .shell
        .service_log
        .as_ref()
        .map(|open| log_fingerprint(Some(open)));
    let scene = {
        let track = world.non_send::<FrontendTrack>();
        track
            .shell
            .service_log
            .as_ref()
            .map(|_| service_log_panel_scene(&track.shell, &palette))
    };
    let slot = world
        .query_filtered::<Entity, With<ServicesLogPanelSlot>>()
        .iter(world)
        .next();
    let Some(slot) = slot else {
        return;
    };
    let stale: Vec<Entity> = world
        .get::<bevy::ecs::hierarchy::Children>(slot)
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    let mut commands = world.commands();
    for entity in stale {
        commands.entity(entity).despawn();
    }
    if let Some(scene) = scene {
        let entity = commands.spawn_scene(scene).id();
        commands.entity(slot).add_one_related::<ChildOf>(entity);
    }
    world.resource_mut::<ServicesLogRenderState>().rendered = fingerprint;
}
