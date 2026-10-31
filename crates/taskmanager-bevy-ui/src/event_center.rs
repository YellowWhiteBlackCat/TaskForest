//! Owned event-history inspection over cached shell facts and native controls.
use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::icons::icon_scene;
use crate::pages::alerts::event_history_line;
use crate::pages::alerts::severity::severity_ink;
use crate::palette::{UiPalette, space_4, space_8};
use crate::text_selection::ClipboardPort;
use crate::window::{Role, TextRole};
use crate::window_surface::{
    WindowSurface, WindowSurfaceChanged, WindowSurfaceCommand, WindowSurfaceKind,
    WindowSurfaceState, modal_scene,
};
use bevy::app::App;
use bevy::ecs::{
    component::Component,
    event::Event,
    hierarchy::Children,
    observer::On,
    resource::Resource,
    system::{Commands, NonSendMut, Query, Res, ResMut},
};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::ui::{
    AlignItems, BackgroundColor, BorderColor, BorderRadius, FlexDirection, Node, UiRect, Val,
    percent, px, rem, widget::Text,
};
use bevy::ui_widgets::{Activate, Button};
use taskforest_core::core::alerts::{AlertEvent, AlertEventKind, export_alert_events_json};
use taskmanager_application::i18n::t;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};
use taskmanager_ui_contract::IconId;

/// The timeline's connector width in px — a hairline is a real geometry
/// contract, not a layout length.
const TIMELINE_TRACK_PX: f32 = 1.0;
/// The timeline node's ring width in px — a ring is a real geometry
/// contract, not a layout length.
const TIMELINE_RING_PX: f32 = 2.0;
#[derive(Resource, Default)]
pub(crate) struct EventCenterState(pub(crate) Option<AlertEventKind>);
/// Marker for one timeline node ring.
#[derive(Component, Clone, Default)]
pub(crate) struct TimelineNode;
/// Marker for the connector that joins two timeline nodes.
#[derive(Component, Clone, Default)]
pub(crate) struct TimelineConnector;
#[derive(Component, Clone, Default)]
pub(crate) struct EventControl(pub(crate) EventAction);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EventAction {
    #[default]
    Open,
    Close,
    Filter(Option<AlertEventKind>),
    Clear,
    Export,
}
#[derive(Event, Clone)]
pub(crate) struct EventCommand(pub(crate) EventAction);
pub(crate) fn button(
    label: String,
    action: EventAction,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {Node {min_height:rem(palette.control_height_rem),padding:UiRect::all(px(space_8()))} BackgroundColor({palette.content_bg}) Button EventControl({action}) on(activate) Children [Text(label) TextRole(Role::Caption) Pickable::IGNORE]}
}

/// The audit timeline's node ring and the connector that runs down to the
/// next event. The ring is hollow: the severity's ink is its outline, so
/// the level reads without a second color patch.
fn timeline_axis_scene(ink: bevy::color::Color, palette: &UiPalette, last: bool) -> Box<dyn Scene> {
    let ring_rem = palette.icon_rem * 0.55;
    let connector: Box<dyn Scene> = if last {
        Box::new(bsn! {Node {width:rem(0.0),height:rem(0.0)}})
    } else {
        Box::new(bsn! {
            Node { width: px(TIMELINE_TRACK_PX), min_height: px(0.0), flex_grow: 1.0 }
            BackgroundColor({ palette.border_color })
            TimelineConnector
        })
    };
    Box::new(bsn! {
        Node {
            width: rem(ring_rem),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
        }
        Children [
             Node {
                width: rem(ring_rem),
                height: rem(ring_rem),
                border_radius: BorderRadius::all(Val::Percent(50.0)),
                border: UiRect::all(px(TIMELINE_RING_PX)),
             }
             BorderColor::all(ink)
             TimelineNode --
             @{ connector }
        ]
    })
}

/// One audit event on the vertical timeline: the node ring, the connector
/// and the event's own facts. The severity's ink — the theme's severity
/// palette, not a frontend literal — dresses the ring and the marker icon.
fn timeline_row_scene(event: &AlertEvent, palette: &UiPalette, last: bool) -> Box<dyn Scene> {
    let line = event_history_line(event);
    let observed = format!("{} ms", event.observed_at_ms);
    let ink = severity_ink(event.alert.severity, palette);
    let marker = icon_scene(IconId::Alert, palette, ink);
    let axis = timeline_axis_scene(ink, palette, last);
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            column_gap: px(space_8()),
        }
        Children [
             @{ axis } --
             Node {
                flex_grow: 1.0,
                min_width: px(0.0),
                flex_direction: FlexDirection::Column,
                row_gap: px(space_4()),
                padding: UiRect::bottom(px(space_8())),
             }
             Children [
                  Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(space_8()),
                  }
                  Children [ @{ marker } -- Text(line) TextRole(Role::Body) ] --
                  Text(observed) TextRole(Role::Caption)
             ]
        ]
    })
}
pub(crate) fn scene(
    shell: &ShellApp,
    state: &EventCenterState,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let projection = shell.projection();
    let events: Vec<&AlertEvent> = projection
        .alert_center
        .event_history()
        .iter()
        .rev()
        .filter(|event| state.0.is_none_or(|kind| kind == event.kind))
        .collect();
    let last = events.len().saturating_sub(1);
    let mut rows: Vec<Box<dyn Scene>> = events
        .iter()
        .enumerate()
        .map(|(index, event)| timeline_row_scene(event, palette, index == last) as Box<dyn Scene>)
        .collect();
    if rows.is_empty() {
        rows.push(Box::new(
            bsn! {Text({t("events.empty")}) TextRole(Role::Body)},
        ));
    }
    let body = Box::new(
        bsn! {Node {width:percent(100),flex_direction:FlexDirection::Column,row_gap:px(space_8())} Children [{rows}]},
    );
    let actions: Vec<Box<dyn Scene>> = vec![
        Box::new(button(
            t("common.all").into(),
            EventAction::Filter(None),
            palette,
        )),
        Box::new(button(
            t("events.activated").into(),
            EventAction::Filter(Some(AlertEventKind::Activated)),
            palette,
        )),
        Box::new(button(
            t("events.cleared").into(),
            EventAction::Filter(Some(AlertEventKind::Cleared)),
            palette,
        )),
        Box::new(button(
            t("common.clear").into(),
            EventAction::Clear,
            palette,
        )),
        Box::new(button(
            t("common.export").into(),
            EventAction::Export,
            palette,
        )),
        Box::new(button(
            t("common.close").into(),
            EventAction::Close,
            palette,
        )),
    ];
    modal_scene(
        WindowSurfaceKind::EventCenter,
        t("events.title"),
        body,
        actions,
        palette,
    )
}
fn activate(event: On<Activate>, controls: Query<&EventControl>, mut commands: Commands) {
    if let Ok(control) = controls.get(event.entity) {
        commands.trigger(EventCommand(control.0));
    }
}
fn on_command(
    event: On<EventCommand>,
    surface: Res<WindowSurfaceState>,
    mut state: ResMut<EventCenterState>,
    mut track: NonSendMut<FrontendTrack>,
    mut output: Option<ResMut<ClipboardPort>>,
    mut toast: Option<ResMut<crate::pages::alerts::toast::ExportToast>>,
    mut commands: Commands,
) {
    if event.0 != EventAction::Open && !matches!(surface.0, Some(WindowSurface::EventCenter)) {
        return;
    }
    match event.0 {
        EventAction::Open => {
            state.0 = None;
            commands.trigger(WindowSurfaceCommand::EventCenter);
        }
        EventAction::Close => {
            commands.trigger(WindowSurfaceCommand::Close(WindowSurfaceKind::EventCenter))
        }
        EventAction::Filter(filter) => state.0 = filter,
        EventAction::Clear => {
            track.shell.clear_alert_event_history();
            commands.trigger(crate::input::ShellInteractionApplied);
        }
        EventAction::Export => {
            let projection = track.shell.projection();
            let exported = export_alert_events_json(projection.alert_center.event_history());
            match exported {
                Ok(json) => {
                    if let Some(output) = output.as_mut() {
                        let label = t("events.title");
                        output.request_text(json, label);
                        if let Some(toast) = toast.as_mut() {
                            toast.requested(label);
                        }
                    } else {
                        track.shell.report_notice(
                            FeedbackSource::Clipboard,
                            FeedbackSeverity::Error,
                            FeedbackLifecycle::UntilReplaced,
                            "clipboard unavailable",
                        );
                    }
                }
                Err(error) => track.shell.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    error.to_string(),
                ),
            }
        }
    }
    commands.trigger(WindowSurfaceChanged);
}
fn on_fold(
    _event: On<ShellProjectionFolded>,
    surface: Res<WindowSurfaceState>,
    mut commands: Commands,
) {
    if matches!(surface.0, Some(WindowSurface::EventCenter)) {
        commands.trigger(WindowSurfaceChanged);
    }
}
pub(crate) fn register(app: &mut App) {
    app.init_resource::<EventCenterState>()
        .add_observer(on_command)
        .add_observer(on_fold);
}

#[cfg(test)]
#[path = "../tests/headless/event_center.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/headless/event_center_timeline.rs"]
mod timeline_tests;
