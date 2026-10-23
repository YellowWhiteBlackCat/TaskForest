//! The export confirmation toast.
//!
//! A preset export that reached the system clipboard confirms itself with a
//! lightweight toast: a green badge beside the copied label, floating over
//! the bottom of the content region.
//!
//! The toast is raised by the shell's own clipboard-success notice — the
//! notice the native clipboard write publishes — so it never claims a copy
//! that did not happen. The export action only *requests* the toast; the
//! shell decides whether it is earned.

use crate::app::ContentSlot;
use crate::icons::icon_scene;
use crate::palette::{UiPalette, space_8, space_12};
use crate::scene_attach::attach_child;
use crate::window::{Role, TextRole, WindowPalette};
use bevy::app::{App, PreUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::time::Time;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderColor, BorderRadius, FlexDirection, JustifyContent, Node,
    PositionType, UiRect, Val, px, rem,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;
use taskmanager_shell::{FeedbackSeverity, FeedbackSource};
use taskmanager_ui_contract::IconId;

/// How long the confirmation toast stays on screen.
const TOAST_SECONDS: f32 = 2.5;
/// The toast badge's ring width in px — a ring is a real geometry contract
/// (a hairline outline), not a layout length.
const TOAST_RING_PX: f32 = 2.0;

/// The toast's own state: the in-flight request, the message on screen and
/// the seconds left before it retires.
#[derive(Resource, Default)]
pub(crate) struct ExportToast {
    /// A copy is in flight; the toast waits for the shell's success notice.
    pub(crate) pending: bool,
    /// The success text the in-flight copy must publish. The shell's notice
    /// carries the exported label, so the toast can tell THIS copy's success
    /// from an older notice.
    pub(crate) expected: String,
    /// The toast's message. Empty while no toast is on screen.
    pub(crate) message: String,
    /// The seconds left before the toast retires.
    pub(crate) remaining: f32,
}

impl ExportToast {
    /// Note that a copy of `label` is in flight. The toast itself is raised
    /// by the shell's success notice, never by the request.
    pub(crate) fn requested(&mut self, label: &str) {
        self.pending = true;
        self.expected = format!("{label} {}", t("common.copied"));
    }
}

/// Marker for the mounted toast overlay.
#[derive(Component, Clone, Default)]
pub(crate) struct ExportToastBadge;

/// Track the toast's state: raise it on the shell's clipboard success, retire
/// it after its lifetime.
fn update_toast(
    time: Res<Time>,
    mut toast: ResMut<ExportToast>,
    mut track: Option<NonSendMut<crate::app::FrontendTrack>>,
) {
    if toast.pending {
        let notice = track
            .as_deref_mut()
            .and_then(|track| track.shell.feedback_notice())
            .filter(|notice| notice.source() == FeedbackSource::Clipboard)
            .map(|notice| (notice.severity(), notice.text().to_owned()));
        if let Some((severity, text)) = notice {
            if severity == FeedbackSeverity::Success && text == toast.expected {
                toast.pending = false;
                toast.message = text;
                toast.remaining = TOAST_SECONDS;
            } else if severity != FeedbackSeverity::Success {
                // The copy was refused: the request is spent and no
                // confirmation may be claimed for it.
                toast.pending = false;
            }
        }
    }
    if toast.remaining > 0.0 {
        toast.remaining = (toast.remaining - time.delta_secs()).max(0.0);
        if toast.remaining == 0.0 {
            toast.message.clear();
        }
    }
}

/// Mount or retire the toast overlay. The overlay hangs under the content
/// slot — the shell's content region — so it survives the page remounts the
/// alerts page performs on every folded telemetry batch.
fn mount_toast(
    toast: Res<ExportToast>,
    palette: Option<Res<WindowPalette>>,
    slots: Query<Entity, With<ContentSlot>>,
    badges: Query<Entity, With<ExportToastBadge>>,
    mut commands: Commands,
) {
    let Ok(slot) = slots.single() else {
        return;
    };
    let existing: Vec<Entity> = badges.iter().collect();
    if toast.message.is_empty() {
        for entity in existing {
            commands.entity(entity).despawn();
        }
        return;
    }
    if !existing.is_empty() {
        return;
    }
    let Some(palette) = palette else {
        return;
    };
    let entity = commands
        .spawn_scene(scene(toast.message.clone(), &palette.inner))
        .id();
    attach_child(&mut commands, slot, entity);
}

/// Register the export toast: its state resource and the two systems that
/// raise and retire the overlay.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<ExportToast>()
        .add_systems(PreUpdate, (update_toast, mount_toast).chain());
}

/// The toast overlay: a floating badge row over the bottom of the content
/// region. The badge carries the theme's accent (the product's green) and
/// the copied label; it never intercepts a pointer aimed at the page below.
pub(crate) fn scene(message: String, palette: &UiPalette) -> Box<dyn Scene> {
    let check = icon_scene(IconId::CircleCheck, palette, palette.success_color);
    // The badge's ink is copied out of the palette: a component built by a
    // constructor call reads its arguments inside the scene's own closure,
    // so the value it reads must be owned by that closure.
    let accent = palette.accent;
    // The toast floats half a control height above the bottom edge: the
    // bottom safety band belongs to the budget, not to a clipping accident.
    let inset = palette.control_height_rem * 0.5;
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            right: px(0.0),
            bottom: rem(inset),
            height: rem(palette.control_height_rem * 2.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        ExportToastBadge
        Pickable::IGNORE
        Children [
             Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
                padding: UiRect::horizontal(Val::Px(space_12())),
                border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
             }
             BackgroundColor({ palette.panel_fill })
             Pickable::IGNORE
             Children [
                  Node {
                    width: rem(palette.icon_rem * 1.5),
                    height: rem(palette.icon_rem * 1.5),
                    border_radius: BorderRadius::all(Val::Percent(50.0)),
                    border: UiRect::all(px(TOAST_RING_PX)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                  }
                  BorderColor::all(accent)
                  Children [ @{ check } ] --
                  Text(message) TextRole(Role::Caption)
             ]
        ]
    })
}
