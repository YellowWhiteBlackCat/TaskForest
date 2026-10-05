//! One primary product surface and bounded modal geometry for Bevy.

use crate::app::FrontendTrack;
use crate::first_run_modal::SetupState;
use crate::input::ShellInteractionApplied;
use crate::menu_modal::{ActionMenuContext, MenuModal, MenuModalChanged};
use crate::pages::performance::sidebar_editor::{self, SidebarState};
use crate::pages::processes::menu::ProcessMenuCtx;
use crate::pages::services::menu::ServiceMenuCtx;
use crate::pages::sessions::menu::SessionMenuCtx;
use crate::pages::settings::ThemePreferences;
use crate::pages::startup::menu::StartupMenuCtx;
use crate::pages::system::diagnostic_modal::DiagnosticRuntime;
use crate::palette::{UiPalette, space_8, space_24};
use crate::widgets::scene_paint::ScenePaint;
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};
use bevy::app::{App, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSend, NonSendMut, Query, Res, ResMut, SystemParam};
use bevy::input::keyboard::KeyCode;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::ui::UiSystems;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, FlexWrap, JustifyContent, Node,
    Overflow, PositionType, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::ScrollArea;
use std::marker::PhantomData;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_shell::presentation::system_information::{SystemInformationGroup, groups};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WindowSurfaceKind {
    SidebarDevices,
    SavedViews,
    About,
    SystemInformation,
    FirstRun,
    #[default]
    Diagnostic,
}
#[derive(Clone, Debug)]
pub(crate) enum WindowSurface {
    SidebarDevices,
    SavedViews,
    About,
    SystemInformation(Vec<SystemInformationGroup>),
    FirstRun,
    Diagnostic(DiagnosticBundleUiState),
}
impl WindowSurface {
    pub(crate) fn kind(&self) -> WindowSurfaceKind {
        match self {
            Self::SavedViews => WindowSurfaceKind::SavedViews,
            Self::SidebarDevices => WindowSurfaceKind::SidebarDevices,
            Self::About => WindowSurfaceKind::About,
            Self::SystemInformation(_) => WindowSurfaceKind::SystemInformation,
            Self::FirstRun => WindowSurfaceKind::FirstRun,
            Self::Diagnostic(_) => WindowSurfaceKind::Diagnostic,
        }
    }
}
#[derive(Resource, Default)]
pub(crate) struct WindowSurfaceState(pub(crate) Option<WindowSurface>);
impl WindowSurfaceState {
    pub(crate) fn diagnostic(&self) -> Option<&DiagnosticBundleUiState> {
        match &self.0 {
            Some(WindowSurface::Diagnostic(state)) => Some(state),
            _ => None,
        }
    }
    pub(crate) fn diagnostic_mut(&mut self) -> Option<&mut DiagnosticBundleUiState> {
        match &mut self.0 {
            Some(WindowSurface::Diagnostic(state)) => Some(state),
            _ => None,
        }
    }
}
#[derive(Event)]
pub(crate) struct WindowSurfaceChanged;
#[derive(Event, Clone, Copy)]
pub(crate) enum WindowSurfaceCommand {
    SidebarDevices,
    SavedViews,
    About,
    SystemInformation,
    FirstRun,
    Close(WindowSurfaceKind),
}
#[derive(Component, Clone, Default)]
pub(crate) struct WindowSurfaceOverlay(pub(crate) WindowSurfaceKind);
#[derive(Component, Clone, Default)]
pub(crate) struct ModalHeading;
#[derive(Component, Clone, Default)]
pub(crate) struct ModalBody;
#[derive(Component, Clone, Default)]
pub(crate) struct ModalFooter;

#[derive(Resource, Default)]
struct SurfacePaint {
    dirty: bool,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<WindowSurfaceState>()
        .init_resource::<SidebarState>()
        .init_resource::<SurfacePaint>()
        .add_observer(on_command)
        .add_observer(request_paint)
        .add_systems(
            PostUpdate,
            paint_surface.in_set(ScenePaint).before(UiSystems::Prepare),
        );
}
#[derive(SystemParam)]
pub(crate) struct SurfaceAccess<'w> {
    pub(crate) state: ResMut<'w, WindowSurfaceState>,
    pub(crate) diagnostic: Option<ResMut<'w, DiagnosticRuntime>>,
    pub(crate) track: Option<NonSendMut<'w, FrontendTrack>>,
    process_menu: Option<ResMut<'w, MenuModal<ProcessMenuCtx>>>,
    service_menu: Option<ResMut<'w, MenuModal<ServiceMenuCtx>>>,
    startup_menu: Option<ResMut<'w, MenuModal<StartupMenuCtx>>>,
    session_menu: Option<ResMut<'w, MenuModal<SessionMenuCtx>>>,
}
fn on_command(
    command: On<WindowSurfaceCommand>,
    mut access: SurfaceAccess,
    prefs: Option<Res<ThemePreferences>>,
    setup: Option<Res<SetupState>>,
    mut commands: Commands,
) {
    match *command.event() {
        WindowSurfaceCommand::SavedViews => {
            show(&mut access, &mut commands, WindowSurface::SavedViews)
        }
        WindowSurfaceCommand::SidebarDevices => {
            show(&mut access, &mut commands, WindowSurface::SidebarDevices)
        }
        WindowSurfaceCommand::About => show(&mut access, &mut commands, WindowSurface::About),
        WindowSurfaceCommand::SystemInformation => {
            let appearance = prefs
                .as_ref()
                .and_then(|prefs| prefs.observed_appearance)
                .unwrap_or_default();
            let facts = access
                .track
                .as_ref()
                .map(|track| {
                    groups(
                        track
                            .shell
                            .projection()
                            .hardware
                            .as_ref()
                            .unwrap_or(&HardwareInfo::default()),
                        appearance,
                    )
                })
                .unwrap_or_default();
            show(
                &mut access,
                &mut commands,
                WindowSurface::SystemInformation(facts),
            );
        }
        WindowSurfaceCommand::FirstRun
            if setup
                .as_ref()
                .is_some_and(|state| state.0.view().info.is_some()) =>
        {
            show(&mut access, &mut commands, WindowSurface::FirstRun);
        }
        WindowSurfaceCommand::FirstRun => {}
        WindowSurfaceCommand::Close(kind) => close(&mut access, &mut commands, kind),
    }
}
fn close_menu<Ctx: ActionMenuContext>(
    menu: Option<&mut MenuModal<Ctx>>,
    track: Option<&mut FrontendTrack>,
    commands: &mut Commands,
) {
    if let Some(menu) = menu {
        if let Some(track) = track {
            let _ = menu.drive(&mut track.shell, KeyCode::Escape, &mut Vec::new());
        }
        commands.trigger(MenuModalChanged::<Ctx>(false, PhantomData));
    }
}
pub(crate) fn show(access: &mut SurfaceAccess, commands: &mut Commands, surface: WindowSurface) {
    if let Some(runtime) = access.diagnostic.as_mut() {
        runtime.close();
    }
    close_menu(
        access.process_menu.as_deref_mut(),
        access.track.as_deref_mut(),
        commands,
    );
    close_menu(
        access.service_menu.as_deref_mut(),
        access.track.as_deref_mut(),
        commands,
    );
    close_menu(
        access.startup_menu.as_deref_mut(),
        access.track.as_deref_mut(),
        commands,
    );
    close_menu(
        access.session_menu.as_deref_mut(),
        access.track.as_deref_mut(),
        commands,
    );
    if let Some(track) = access.track.as_mut() {
        track.shell.dismiss_overlay();
        track.shell.close_service_log();
        track.shell.dismiss_informational_overlay();
        track.shell.close_search();
    }
    access.state.0 = Some(surface);
    commands.trigger(WindowSurfaceChanged);
    commands.trigger(ShellInteractionApplied);
}
pub(crate) fn close(
    access: &mut SurfaceAccess,
    commands: &mut Commands,
    expected: WindowSurfaceKind,
) {
    if !access
        .state
        .0
        .as_ref()
        .is_some_and(|surface| surface.kind() == expected)
    {
        return;
    }
    if let Some(runtime) = access.diagnostic.as_mut() {
        runtime.close();
    }
    access.state.0 = None;
    commands.trigger(WindowSurfaceChanged);
}
fn request_paint(_event: On<WindowSurfaceChanged>, mut paint: ResMut<SurfacePaint>) {
    paint.dirty = true;
}
#[derive(SystemParam)]
struct SurfaceRender<'w, 's> {
    sidebar: Res<'w, SidebarState>,
    saved_views: Option<Res<'w, crate::saved_views::SavedViewsState>>,
    track: Option<NonSend<'w, FrontendTrack>>,
    state: Res<'w, WindowSurfaceState>,
    paint: ResMut<'w, SurfacePaint>,
    palette: Option<Res<'w, WindowPalette>>,
    setup: Option<Res<'w, SetupState>>,
    roots: Query<'w, 's, Entity, With<AppShellRoot>>,
    overlays: Query<'w, 's, Entity, With<WindowSurfaceOverlay>>,
    commands: Commands<'w, 's>,
}
fn paint_surface(mut render: SurfaceRender) {
    let Ok(root) = render.roots.single() else {
        return;
    };
    if !render.paint.dirty && !(render.state.0.is_some() && render.overlays.is_empty()) {
        return;
    }
    let Some(palette) = render.palette.as_ref().map(|palette| &palette.inner) else {
        return;
    };
    let scene: Option<Box<dyn Scene>> = match &render.state.0 {
        Some(WindowSurface::SavedViews) => render
            .saved_views
            .as_ref()
            .map(|state| Box::new(crate::saved_views::scene(state, palette)) as Box<dyn Scene>),
        Some(WindowSurface::SidebarDevices) => render.track.as_ref().map(|track| {
            Box::new(sidebar_editor::scene(
                &track.shell,
                &render.sidebar,
                palette,
            )) as Box<dyn Scene>
        }),
        Some(WindowSurface::About) => Some(Box::new(crate::about_modal::surface_scene(palette))),
        Some(WindowSurface::SystemInformation(facts)) => Some(Box::new(
            crate::system_information_modal::surface_scene(facts, palette),
        )),
        Some(WindowSurface::FirstRun) => render.setup.as_ref().map(|setup| {
            Box::new(crate::first_run_modal::surface_scene(
                setup.0.view(),
                palette,
            )) as Box<dyn Scene>
        }),
        Some(WindowSurface::Diagnostic(state)) => Some(Box::new(
            crate::pages::system::diagnostic_modal::overlay_scene(state, palette),
        )),
        None => None,
    };
    for entity in &render.overlays {
        render.commands.entity(entity).despawn();
    }
    if let Some(scene) = scene {
        let overlay = render.commands.spawn_scene(scene).id();
        render
            .commands
            .entity(root)
            .add_one_related::<ChildOf>(overlay);
    }
    render.paint.dirty = false;
}

/// Header and complete action rows are mandatory; the body alone consumes
/// remaining height and scrolls. Insets protect bottom/right edges.
pub(crate) fn modal_scene(
    kind: WindowSurfaceKind,
    title: &'static str,
    body: Box<dyn Scene>,
    actions: Vec<Box<dyn Scene>>,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center, align_items: AlignItems::Center }
        BackgroundColor({ palette.scrim }) WindowSurfaceOverlay({ kind })
        Children [
            Node { width: percent(90), max_width: px(680.0), height: percent(90), max_height: percent(90), min_height: px(0.0), min_width: px(0.0),
                flex_direction: FlexDirection::Column, row_gap: Val::Px(space_8()), padding: UiRect::all(Val::Px(space_24())), border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)) }
            BackgroundColor({ palette.panel_fill })
            Children [
                Node { min_height: px(palette.control_height_px), flex_shrink: 0.0 } ModalHeading
                Children [ Text(title) TextRole(Role::Heading) ] --
                Node { flex_grow: 1.0, flex_basis: px(0.0), min_height: px(0.0), min_width: px(0.0), overflow: Overflow::scroll_y() }
                ScrollArea ModalBody Children [ @{ body } ] --
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(space_8()), row_gap: Val::Px(space_8()), min_height: px(palette.control_height_px), flex_shrink: 0.0 }
                ModalFooter Children [ { actions } ]
            ]
        ]
    }
}
