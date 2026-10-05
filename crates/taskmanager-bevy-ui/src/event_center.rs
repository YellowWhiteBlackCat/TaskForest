//! Owned event-history inspection over cached shell facts and native controls.
use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::pages::alerts::event_history_line;
use crate::palette::{UiPalette, space_8};
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
use bevy::ui::{BackgroundColor, FlexDirection, Node, UiRect, percent, px, widget::Text};
use bevy::ui_widgets::{Activate, Button};
use taskmanager_application::i18n::t;
use taskmanager_core::core::alerts::{AlertEventKind, export_alert_events_json};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};
#[derive(Resource, Default)]
pub(crate) struct EventCenterState(pub(crate) Option<AlertEventKind>);
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
    bsn! {Node {min_height:px(palette.control_height_px),padding:UiRect::all(px(space_8()))} BackgroundColor({palette.content_bg}) Button EventControl({action}) on(activate) Children [Text(label) TextRole(Role::Caption) Pickable::IGNORE]}
}
pub(crate) fn scene(
    shell: &ShellApp,
    state: &EventCenterState,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let projection = shell.projection();
    let mut rows:Vec<Box<dyn Scene>>=projection.alert_center.event_history().iter().rev().filter(|event|state.0.is_none_or(|kind|kind==event.kind)).map(|event|{
        let line=event_history_line(event);let observed=format!("{} ms",event.observed_at_ms);
        Box::new(bsn! {Node {width:percent(100),flex_shrink:0.0,flex_direction:FlexDirection::Column,row_gap:px(space_8()),padding:UiRect::bottom(px(space_8()))} Children [Text(line) TextRole(Role::Body) -- Text(observed) TextRole(Role::Caption)]}) as Box<dyn Scene>
    }).collect();
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
                        output.request_text(json, t("events.title"));
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
