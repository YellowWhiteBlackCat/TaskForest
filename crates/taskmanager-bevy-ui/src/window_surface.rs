//! One primary product surface and bounded modal geometry for Bevy.

use crate::app::FrontendTrack;
use crate::first_run_modal::SetupState;
use crate::input::ShellInteractionApplied;
use crate::menu_modal::{ActionMenuContext, MenuModal, MenuModalChanged};
use crate::pages::processes::menu::ProcessMenuCtx;
use crate::pages::services::menu::ServiceMenuCtx;
use crate::pages::sessions::menu::SessionMenuCtx;
use crate::pages::settings::ThemePreferences;
use crate::pages::startup::menu::StartupMenuCtx;
use crate::pages::system::diagnostic_modal::DiagnosticRuntime;
use crate::palette::{UiPalette, space_8, space_24};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};
use bevy::app::{App, PostUpdate};
use bevy::ecs::change_detection::Mut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, ResMut};
use bevy::ecs::world::World;
use bevy::input::keyboard::KeyCode;
use bevy::scene::{Scene, WorldSceneExt, bsn};
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
    About,
    SystemInformation,
    FirstRun,
    #[default]
    Diagnostic,
}
#[derive(Clone, Debug)]
pub(crate) enum WindowSurface {
    About,
    SystemInformation(Vec<SystemInformationGroup>),
    FirstRun,
    Diagnostic(DiagnosticBundleUiState),
}
impl WindowSurface {
    pub(crate) fn kind(&self) -> WindowSurfaceKind {
        match self {
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
        .init_resource::<SurfacePaint>()
        .add_observer(on_command)
        .add_observer(request_paint)
        .add_systems(PostUpdate, paint_surface.before(UiSystems::Prepare));
}
fn on_command(command: On<WindowSurfaceCommand>, mut commands: Commands) {
    let command = *command.event();
    commands.queue(move |world: &mut World| match command {
        WindowSurfaceCommand::About => show(world, WindowSurface::About),
        WindowSurfaceCommand::SystemInformation => {
            let appearance = world
                .get_resource::<ThemePreferences>()
                .and_then(|prefs| prefs.observed_appearance)
                .unwrap_or_default();
            let facts = world
                .get_non_send::<FrontendTrack>()
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
            show(world, WindowSurface::SystemInformation(facts));
        }
        WindowSurfaceCommand::FirstRun
            if world
                .get_resource::<SetupState>()
                .is_some_and(|state| state.0.view().info.is_some()) =>
        {
            show(world, WindowSurface::FirstRun)
        }
        WindowSurfaceCommand::FirstRun => {}
        WindowSurfaceCommand::Close(kind) => close(world, kind),
    });
}
fn close_menu<Ctx: ActionMenuContext>(world: &mut World) {
    if world.get_resource::<MenuModal<Ctx>>().is_none() {
        return;
    }
    world.resource_scope(|world, mut menu: Mut<MenuModal<Ctx>>| {
        let mut track = world.non_send_mut::<FrontendTrack>();
        let _ = menu.drive(&mut track.shell, KeyCode::Escape, &mut Vec::new());
    });
    world.trigger(MenuModalChanged::<Ctx>(false, PhantomData));
}
pub(crate) fn show(world: &mut World, surface: WindowSurface) {
    if let Some(mut runtime) = world.get_resource_mut::<DiagnosticRuntime>() {
        runtime.close();
    }
    close_menu::<ProcessMenuCtx>(world);
    close_menu::<ServiceMenuCtx>(world);
    close_menu::<StartupMenuCtx>(world);
    close_menu::<SessionMenuCtx>(world);
    if let Some(mut track) = world.get_non_send_mut::<FrontendTrack>() {
        track.shell.dismiss_overlay();
        track.shell.close_service_log();
        track.shell.dismiss_informational_overlay();
        track.shell.close_search();
    }
    world.resource_mut::<WindowSurfaceState>().0 = Some(surface);
    world.trigger(WindowSurfaceChanged);
    world.trigger(ShellInteractionApplied);
}
pub(crate) fn close(world: &mut World, expected: WindowSurfaceKind) {
    if world
        .resource::<WindowSurfaceState>()
        .0
        .as_ref()
        .map(WindowSurface::kind)
        != Some(expected)
    {
        return;
    }
    if let Some(mut runtime) = world.get_resource_mut::<DiagnosticRuntime>() {
        runtime.close();
    }
    world.resource_mut::<WindowSurfaceState>().0 = None;
    world.trigger(WindowSurfaceChanged);
}
fn request_paint(_event: On<WindowSurfaceChanged>, mut paint: ResMut<SurfacePaint>) {
    paint.dirty = true;
}
fn paint_surface(world: &mut World) {
    let root = world
        .query_filtered::<Entity, With<AppShellRoot>>()
        .single(world)
        .ok();
    let Some(root) = root else {
        return;
    };
    let overlays: Vec<_> = world
        .query_filtered::<Entity, With<WindowSurfaceOverlay>>()
        .iter(world)
        .collect();
    let surface = world.resource::<WindowSurfaceState>().0.clone();
    if !world.resource::<SurfacePaint>().dirty && !(surface.is_some() && overlays.is_empty()) {
        return;
    }
    let Some(palette) = world
        .get_resource::<WindowPalette>()
        .map(|palette| palette.inner.clone())
    else {
        return;
    };
    let scene: Option<Box<dyn Scene>> = match &surface {
        Some(WindowSurface::About) => Some(Box::new(crate::about_modal::surface_scene(&palette))),
        Some(WindowSurface::SystemInformation(facts)) => Some(Box::new(
            crate::system_information_modal::surface_scene(facts, &palette),
        )),
        Some(WindowSurface::FirstRun) => world.get_resource::<SetupState>().map(|setup| {
            Box::new(crate::first_run_modal::surface_scene(
                setup.0.view(),
                &palette,
            )) as Box<dyn Scene>
        }),
        Some(WindowSurface::Diagnostic(state)) => Some(Box::new(
            crate::pages::system::diagnostic_modal::overlay_scene(state, &palette),
        )),
        None => None,
    };
    for entity in overlays {
        world.despawn(entity);
    }
    if let Some(scene) = scene {
        let overlay = match world.spawn_scene(scene) {
            Ok(overlay) => overlay.id(),
            Err(error) => {
                eprintln!("taskforest-b: product surface could not mount: {error}");
                return;
            }
        };
        world.entity_mut(root).add_one_related::<ChildOf>(overlay);
    }
    world.resource_mut::<SurfacePaint>().dirty = false;
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
