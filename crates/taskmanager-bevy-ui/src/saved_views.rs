//! Saved-view controls declare resources, owned clipboard requests and component queries.
use crate::app::{FrontendTrack, Page, Route, RouteChanged, SharedRuntimeHandle};
use crate::input::ShellInteractionApplied;
use crate::pages::performance::sidebar_editor::SidebarState;
use crate::pages::processes::columns_modal::ProcessHiddenColumns;
use crate::palette::{UiPalette, space_8};
use crate::text_selection::ClipboardPort;
use crate::window::{DemoMode, Role, TextRole};
use crate::window_surface::{
    WindowSurface, WindowSurfaceChanged, WindowSurfaceCommand, WindowSurfaceKind,
    WindowSurfaceState, modal_scene,
};
use bevy::app::{App, PreUpdate};
use bevy::clipboard::{Clipboard, ClipboardRead};
use bevy::ecs::{
    component::Component,
    event::Event,
    hierarchy::Children,
    observer::On,
    resource::Resource,
    system::{Commands, NonSendMut, Query, Res, ResMut, SystemParam},
};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::{BackgroundColor, FlexDirection, FlexWrap, Node, UiRect, percent, px, widget::Text};
use bevy::ui_widgets::{Activate, Button};
use std::sync::Arc;
use taskmanager_application::{AppAction, AppPage, ConfigSubmitError, i18n::t};
use taskmanager_core::core::config::ProcessViewPresetConfig;
use taskmanager_shell::saved_views::{
    SavedViewPreset, SavedViewTransferFeedback, default_built_in_presets, export_saved_views_json,
    feedback_text, import_saved_views_json, preset_to_config, restore_saved_views, review_rows,
    save_current_view, sort_from_token, sort_token,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, SortDir};

#[derive(Resource)]
pub(crate) struct SavedViewsState {
    pub(crate) rows: Vec<SavedViewPreset>,
    next_id: u64,
    applied: Vec<ProcessViewPresetConfig>,
    feedback: Option<SavedViewTransferFeedback>,
}
impl Default for SavedViewsState {
    fn default() -> Self {
        Self {
            rows: default_built_in_presets(),
            next_id: 4,
            applied: Vec::new(),
            feedback: None,
        }
    }
}
#[derive(Resource, Default)]
struct PendingImport(Option<ClipboardRead>);
#[derive(Component, Clone, Default)]
pub(crate) struct SavedViewControl(pub(crate) SavedViewAction);
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum SavedViewAction {
    #[default]
    Open,
    Close,
    Apply(u64),
    Save,
    Remove(u64),
    Export,
    Import,
}

pub(crate) fn button(
    label: String,
    action: SavedViewAction,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! { Node {min_height:px(palette.control_height_px),padding:UiRect::all(px(space_8()))} BackgroundColor({palette.content_bg}) Button SavedViewControl({action}) on(activate) Children [Text(label) TextRole(Role::Caption)] }
}
pub(crate) fn scene(state: &SavedViewsState, palette: &UiPalette) -> impl Scene + use<> {
    let rows:Vec<Box<dyn Scene>>=review_rows(&state.rows).map(|preset| {
        let mut actions:Vec<Box<dyn Scene>>=vec![Box::new(button(t("common.apply").into(),SavedViewAction::Apply(preset.id),palette))];
        if preset.is_user_saved() {actions.push(Box::new(button(t("common.remove").into(),SavedViewAction::Remove(preset.id),palette)));}
        let caption=format!("{} · {} {}",preset.filter.label(),preset.sort_col.label(),if preset.sort_asc {"ASC"} else {"DESC"});
        Box::new(bsn! {Node {width:percent(100),flex_shrink:0.0,flex_direction:FlexDirection::Column,row_gap:px(space_8()),padding:UiRect::bottom(px(space_8()))}
            Children [Text({preset.display_name()}) TextRole(Role::Body) -- Text(caption) TextRole(Role::Caption) -- Node {flex_direction:FlexDirection::Row,flex_wrap:FlexWrap::Wrap,column_gap:px(space_8())} Children [{actions}]]}) as Box<dyn Scene>
    }).collect();
    let feedback = state
        .feedback
        .map(feedback_text)
        .unwrap_or_else(|| t("saved_views.help").into());
    let body = Box::new(
        bsn! {Node {width:percent(100),flex_direction:FlexDirection::Column,row_gap:px(space_8())} Children [Text(feedback) TextRole(Role::Caption) -- {rows}]},
    );
    let actions: Vec<Box<dyn Scene>> = vec![
        Box::new(button(
            t("saved_views.save_current").into(),
            SavedViewAction::Save,
            palette,
        )),
        Box::new(button(
            t("common.export").into(),
            SavedViewAction::Export,
            palette,
        )),
        Box::new(button(
            t("common.import").into(),
            SavedViewAction::Import,
            palette,
        )),
        Box::new(button(
            t("common.close").into(),
            SavedViewAction::Close,
            palette,
        )),
    ];
    modal_scene(
        WindowSurfaceKind::SavedViews,
        t("saved_views.title"),
        body,
        actions,
        palette,
    )
}

#[derive(SystemParam)]
struct SavedViewsAccess<'w, 's> {
    state: ResMut<'w, SavedViewsState>,
    track: NonSendMut<'w, FrontendTrack>,
    hidden: ResMut<'w, ProcessHiddenColumns>,
    route: ResMut<'w, Route>,
    prefs: ResMut<'w, SidebarState>,
    runtime: Option<Res<'w, SharedRuntimeHandle>>,
    demo: Option<Res<'w, DemoMode>>,
    clipboard: Option<ResMut<'w, Clipboard>>,
    output: Option<ResMut<'w, ClipboardPort>>,
    pending: ResMut<'w, PendingImport>,
    commands: Commands<'w, 's>,
}
fn report(access: &mut SavedViewsAccess, error: impl std::fmt::Display) {
    access.track.shell.report_notice(
        FeedbackSource::Settings,
        FeedbackSeverity::Warning,
        FeedbackLifecycle::UntilReplaced,
        format!("Saved view operation: {error}"),
    );
}
fn publish(access: &mut SavedViewsAccess, rows: Vec<SavedViewPreset>, next_id: u64) {
    let mut config = (*access.prefs.0).clone();
    config.saved_process_views = rows.iter().filter_map(preset_to_config).collect();
    if access.demo.is_some() {
        access.prefs.0 = Arc::new(config);
        access.state.rows = rows;
        access.state.next_id = next_id;
        access.commands.trigger(WindowSurfaceChanged);
        return;
    }
    let submitted = access
        .runtime
        .as_deref()
        .and_then(|runtime| {
            let guard = runtime.shared.lock_config();
            let client = guard.as_ref()?;
            let mut current = (**client.snapshot()?).clone();
            current.saved_process_views = config.saved_process_views;
            Some(client.try_submit(current))
        })
        .unwrap_or(Err(ConfigSubmitError::NotReady));
    if let Err(error) = submitted {
        report(access, error);
    }
}
#[derive(Event, Clone)]
pub(crate) struct SavedViewCommand(pub(crate) SavedViewAction);
fn activate(event: On<Activate>, controls: Query<&SavedViewControl>, mut commands: Commands) {
    if let Ok(control) = controls.get(event.entity) {
        commands.trigger(SavedViewCommand(control.0.clone()));
    }
}
fn on_command(event: On<SavedViewCommand>, mut access: SavedViewsAccess) {
    let action = event.0.clone();
    match action {
        SavedViewAction::Open => access.commands.trigger(WindowSurfaceCommand::SavedViews),
        SavedViewAction::Close => access
            .commands
            .trigger(WindowSurfaceCommand::Close(WindowSurfaceKind::SavedViews)),
        SavedViewAction::Apply(id) => {
            if let Some(preset) = access
                .state
                .rows
                .iter()
                .find(|entry| entry.id == id)
                .cloned()
            {
                let _ = access
                    .track
                    .shell
                    .apply_action(AppAction::SelectPage(AppPage::Applications));
                access.track.shell.set_process_status_filter(preset.filter);
                access.track.shell.set_sort_column(preset.sort_col);
                access.track.shell.process_sort.1 = if preset.sort_asc {
                    SortDir::Asc
                } else {
                    SortDir::Desc
                };
                access.hidden.0 = preset
                    .hidden_cols
                    .into_iter()
                    .map(sort_token)
                    .map(str::to_owned)
                    .collect();
                access.route.page = Page::Processes;
                access.commands.trigger(RouteChanged);
                access.commands.trigger(ShellInteractionApplied);
                access
                    .commands
                    .trigger(WindowSurfaceCommand::Close(WindowSurfaceKind::SavedViews));
            }
        }
        SavedViewAction::Save => {
            let hidden = access
                .hidden
                .0
                .iter()
                .filter_map(|token| sort_from_token(token))
                .collect();
            let preset = SavedViewPreset::restored(
                t("saved_views.custom_name").replace(
                    "{index}",
                    &access.state.next_id.saturating_sub(3).to_string(),
                ),
                access.track.shell.process_status_filter,
                access.track.shell.process_sort.0,
                access.track.shell.process_sort.1 == SortDir::Asc,
                hidden,
            );
            let mut rows = access.state.rows.clone();
            let mut next = access.state.next_id;
            match save_current_view(&mut rows, &mut next, preset) {
                Ok(_) => publish(&mut access, rows, next),
                Err(error) => report(&mut access, error),
            }
        }
        SavedViewAction::Remove(id) => {
            if access
                .state
                .rows
                .iter()
                .any(|entry| entry.id == id && entry.is_user_saved())
            {
                let mut rows = access.state.rows.clone();
                rows.retain(|entry| entry.id != id);
                let next = access.state.next_id;
                publish(&mut access, rows, next);
            }
        }
        SavedViewAction::Export => match export_saved_views_json(&access.state.rows) {
            Ok(json) => {
                if let Some(output) = access.output.as_mut() {
                    output.request_text(json, t("saved_views.title"));
                } else {
                    report(&mut access, "clipboard unavailable");
                }
            }
            Err(error) => report(&mut access, error),
        },
        SavedViewAction::Import => {
            if access.pending.0.is_none() {
                if let Some(clipboard) = access.clipboard.as_mut() {
                    access.pending.0 = Some(clipboard.fetch_text());
                } else {
                    report(&mut access, "clipboard unavailable");
                }
            }
        }
    }
    access.commands.trigger(WindowSurfaceChanged);
}
fn sync_config(
    prefs: Res<SidebarState>,
    mut state: ResMut<SavedViewsState>,
    mut commands: Commands,
) {
    if state.applied != prefs.0.saved_process_views {
        let SavedViewsState {
            rows,
            next_id,
            applied,
            ..
        } = &mut *state;
        if restore_saved_views(rows, next_id, &prefs.0.saved_process_views).is_ok() {
            *applied = prefs.0.saved_process_views.clone();
            commands.trigger(WindowSurfaceChanged);
        }
    }
}
fn poll_import(surface: Res<WindowSurfaceState>, mut access: SavedViewsAccess) {
    if !matches!(surface.0, Some(WindowSurface::SavedViews)) {
        access.pending.0 = None;
        return;
    }
    let Some(read) = access.pending.0.as_mut() else {
        return;
    };
    let Some(result) = read.poll_result() else {
        return;
    };
    access.pending.0 = None;
    match result {
        Ok(json) => {
            let mut rows = access.state.rows.clone();
            let mut next = access.state.next_id;
            match import_saved_views_json(&mut rows, &mut next, &json) {
                Ok(summary) => {
                    publish(&mut access, rows, next);
                    access.state.feedback = Some(SavedViewTransferFeedback::Imported(summary));
                }
                Err(error) => {
                    access.state.feedback = Some(SavedViewTransferFeedback::ImportInvalid);
                    report(&mut access, error);
                }
            }
        }
        Err(error) => report(&mut access, error),
    }
    access.commands.trigger(WindowSurfaceChanged);
}
pub(crate) fn register(app: &mut App) {
    app.init_resource::<SavedViewsState>()
        .init_resource::<PendingImport>()
        .init_resource::<ProcessHiddenColumns>()
        .add_observer(on_command)
        .add_systems(PreUpdate, (sync_config, poll_import));
}

#[cfg(test)]
#[path = "../tests/headless/saved_views.rs"]
mod tests;
