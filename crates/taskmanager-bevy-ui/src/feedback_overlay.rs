//! Shell-owned feedback notice rendered as a non-modal floating card.

use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::drain::{FeedbackChanged, feedback_changed};
use crate::palette::{UiPalette, space_12};
use crate::scene_attach::attach_child;
use crate::window::overlay_root::OverlayRoot;
use crate::window::{CustomTextInk, FeedbackLine, Role, TextRole, WindowPalette};
use bevy::app::{App, PostStartup};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, NonSend, Query, Res};
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::text::{LineBreak, TextColor, TextLayout};
use bevy::ui::BorderColor;
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, BoxShadow, FlexDirection, Node, PositionType, UiRect, percent,
    px, rem,
};
use bevy::ui::widget::Text;
use taskmanager_shell::FeedbackSource;
use taskmanager_shell::FeedbackTone;
use taskmanager_theme::GlassSurfaceRole;

/// Renderer bookkeeping for the one mounted shell notice.
///
/// The notice itself remains owned by `ShellApp`; this component only lets the
/// mount observer avoid rebuilding an unchanged card.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FeedbackToast {
    pub(crate) text: String,
    pub(crate) source: Option<FeedbackSource>,
    pub(crate) tone: Option<FeedbackTone>,
}

impl FeedbackToast {
    fn from_projection(projection: &FeedbackChanged) -> Self {
        Self {
            text: projection.text.clone(),
            source: projection.notice.map(|notice| notice.source),
            tone: projection.notice.map(|notice| notice.tone),
        }
    }
}

fn tone_ink(tone: FeedbackTone, palette: &UiPalette) -> bevy::color::Color {
    match tone {
        FeedbackTone::Neutral => palette.body_color,
        FeedbackTone::Positive => palette.success_color,
        FeedbackTone::Caution => palette.warning_color,
        FeedbackTone::Danger => palette.danger_color,
    }
}

fn show_projection(
    projection: &FeedbackChanged,
    page: Page,
    palette: Option<&WindowPalette>,
    roots: &Query<Entity, With<OverlayRoot>>,
    overlays: &Query<(Entity, &FeedbackToast)>,
    commands: &mut Commands,
) {
    let existing: Vec<(Entity, FeedbackToast)> = overlays
        .iter()
        .map(|(entity, toast)| (entity, toast.clone()))
        .collect();
    let visible =
        page == Page::Performance && projection.notice.is_some() && !projection.text.is_empty();
    let next = FeedbackToast::from_projection(projection);
    if visible && existing.iter().any(|(_, current)| current == &next) {
        return;
    }
    for (entity, _) in existing {
        commands.entity(entity).despawn();
    }
    if !visible {
        return;
    }
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let entity = commands
        .spawn_scene(toast_scene(projection.clone(), &palette.inner))
        .id();
    attach_child(commands, root, entity);
}

fn on_feedback_changed(
    changed: On<FeedbackChanged>,
    route: Res<Route>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<OverlayRoot>>,
    overlays: Query<(Entity, &FeedbackToast)>,
    mut commands: Commands,
) {
    show_projection(
        changed.event(),
        route.page,
        palette.as_deref(),
        &roots,
        &overlays,
        &mut commands,
    );
}

fn on_route_changed(
    _changed: On<RouteChanged>,
    track: NonSend<FrontendTrack>,
    route: Res<Route>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<OverlayRoot>>,
    overlays: Query<(Entity, &FeedbackToast)>,
    mut commands: Commands,
) {
    let projection = feedback_changed(&track.shell);
    show_projection(
        &projection,
        route.page,
        palette.as_deref(),
        &roots,
        &overlays,
        &mut commands,
    );
}

fn sync_after_mount(
    track: NonSend<FrontendTrack>,
    route: Res<Route>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<OverlayRoot>>,
    overlays: Query<(Entity, &FeedbackToast)>,
    mut commands: Commands,
) {
    let projection = feedback_changed(&track.shell);
    show_projection(
        &projection,
        route.page,
        palette.as_deref(),
        &roots,
        &overlays,
        &mut commands,
    );
}

/// Register shell feedback and route observers for the floating surface.
pub(crate) fn register(app: &mut App) {
    app.add_observer(on_feedback_changed)
        .add_observer(on_route_changed)
        .add_systems(PostStartup, sync_after_mount);
}

fn toast_scene(projection: FeedbackChanged, palette: &UiPalette) -> Box<dyn Scene> {
    let tone = projection
        .notice
        .map_or(FeedbackTone::Neutral, |notice| notice.tone);
    let ink = tone_ink(tone, palette);
    let role = GlassSurfaceRole::Toast;
    let tier = role.tier();
    let material = crate::glass::surface_material(role, palette);
    let message_text = projection.text.clone();
    let notice = projection.notice;
    let source = notice.map(|value| value.source);
    let tone = notice.map(|value| value.tone);
    let inset = palette.control_height_rem * 0.5;
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            right: px(space_12()),
            bottom: rem(inset),
            width: rem(32.0),
            max_width: percent(90),
            min_width: px(0.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(rem(palette.control_height_rem * 0.25)),
            border: UiRect::all(rem(material.edge_width_rem)),
            border_radius: BorderRadius::all(rem(material.radius_rem)),
        }
        FeedbackToast { text: message_text, source, tone }
        BackgroundColor({ material.fill })
        BorderColor::all(ink)
        BoxShadow({ material.shadows })
        crate::glass::OpticalGlassCard { tier: {tier}, enabled: true }
        Pickable::IGNORE
        Children [
            @{ material.grain } --
            Text({ projection.text.clone() }) TextRole(Role::Body) TextColor(ink) CustomTextInk
            TextLayout { linebreak: LineBreak::WordOrCharacter }
            FeedbackLine Pickable::IGNORE
        ]
    })
}

#[cfg(test)]
#[path = "../tests/headless/feedback_overlay.rs"]
mod tests;
