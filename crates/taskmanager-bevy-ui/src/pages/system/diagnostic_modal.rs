//! Diagnostic report preview and failure modals for the Bevy frontend.

use bevy::app::App;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, PositionType,
    UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;

use crate::palette::{UiPalette, space_8, space_24};
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticModalKind {
    Preview,
    Failure,
}

#[derive(Event, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticModalChanged(pub(crate) Option<DiagnosticModalKind>);

#[derive(Component, Clone, Default)]
pub(crate) struct DiagnosticModalOverlay;

pub(crate) fn register(app: &mut App) {
    app.add_observer(on_diagnostic_modal_changed);
}

fn on_diagnostic_modal_changed(
    changed: On<DiagnosticModalChanged>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<DiagnosticModalOverlay>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    let Some(kind) = changed.event().0 else {
        return;
    };
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let overlay = commands
        .spawn_scene(diagnostic_overlay_scene(kind, &palette.inner))
        .id();
    commands.entity(root).add_one_related::<ChildOf>(overlay);
}

fn diagnostic_overlay_scene(kind: DiagnosticModalKind, palette: &UiPalette) -> impl Scene + use<> {
    let panel = match kind {
        DiagnosticModalKind::Preview => preview_panel_scene(palette),
        DiagnosticModalKind::Failure => failure_panel_scene(palette),
    };
    let scrim = palette.scrim;
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        BackgroundColor({ scrim })
        DiagnosticModalOverlay
        Children [
            @{ panel }
        ]
    }
}

fn preview_panel_scene(palette: &UiPalette) -> Box<dyn Scene> {
    let title = t("diag.preview_title").to_owned();
    let summary = "# TaskForest Diagnostic Report\n\n- System telemetry: OK\n- Hardware topology: Verified\n- Kernel version: Linux 6.18 LTS".to_owned();
    let hint = t("diag.preview_hint").to_owned();
    let radius = palette.panel_radius_px;
    Box::new(bsn! {
        Node {
            width: px(580.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            Text(title) TextRole(Role::Heading) --
            Text(summary) TextRole(Role::Mono) --
            Text(hint) TextRole(Role::Caption)
        ]
    })
}

fn failure_panel_scene(palette: &UiPalette) -> Box<dyn Scene> {
    let title = t("diag.failure_title").to_owned();
    let desc = t("diag.failure_desc").to_owned();
    let reason =
        "Error: Diagnostic collector timed out waiting for platform provider response.".to_owned();
    let radius = palette.panel_radius_px;
    Box::new(bsn! {
        Node {
            width: px(480.0),
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_8()),
            padding: UiRect::all(Val::Px(space_24())),
            border_radius: BorderRadius::all(Val::Px(radius)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [
            Text(title) TextRole(Role::Heading) --
            Text(desc) TextRole(Role::Body) --
            Text(reason) TextRole(Role::Caption)
        ]
    })
}
