//! The active-alert banner's damped spring reveal.
//!
//! A firing alert opens as a roller blind: the banner frame's height eases
//! from zero to its slot through a damped spring, so the alert arrives with a
//! motion instead of a hard displacement. The spring lives on the banner
//! entity as a typed component; one system integrates it and writes the
//! frame's height — the scene never animates by itself.
//!
//! The reveal belongs to the *arrival* of an alert, not to the page's
//! periodic remount: the alerts page remounts on every folded telemetry
//! batch, so the system remembers which alert instances it already opened and
//! leaves an already-known banner settled.

use crate::icons::icon_scene;
use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};
use bevy::app::{App, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::time::Time;
use bevy::ui::UiSystems;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, FlexDirection, JustifyContent, Node, Overflow, PositionType,
    UiRect, Val, percent, px, rem,
};
use bevy::ui::widget::Text;
use std::collections::VecDeque;
use taskmanager_core::core::alerts::Alert;
use taskmanager_ui_contract::IconId;

/// The angular frequency of the banner spring (rad/s). The spring's natural
/// period is roughly half a second — long enough to read as a motion, short
/// enough that a firing alert is never late.
const SPRING_FREQUENCY: f32 = 12.0;
/// The damping ratio of the reveal. Below 1 the banner overshoots a little
/// before it settles — the "elastic" part of the motion.
const DAMPING_RATIO: f32 = 0.55;
/// The largest integration step. A long frame (a stalled compositor, a
/// debugger pause) must not make the spring explode.
const MAX_STEP_SECS: f32 = 1.0 / 30.0;
/// The settle tolerance in rem: below this distance AND velocity the banner
/// is pinned to its target, so the blind never jitters around the slot.
const SETTLE_EPSILON: f32 = 0.001;
/// How many alert instances the spring remembers. The window only needs the
/// recent arrivals; an unbounded set would grow with every alert ever fired.
const OPENED_MEMORY: usize = 128;

/// The banner frame's spring state, in rem. The value is the frame's height;
/// the target is the slot the banner settles into.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BannerSpring {
    /// The frame's current height in rem.
    pub(crate) value: f32,
    /// The frame's current velocity in rem/s.
    pub(crate) velocity: f32,
    /// The frame's settled height in rem.
    pub(crate) target: f32,
    /// Whether the spring has taken ownership of this banner yet.
    pub(crate) initialized: bool,
}

impl BannerSpring {
    /// One integration step toward the target. The sub-stepped semi-implicit
    /// Euler form keeps the spring stable at any frame time.
    fn advance(&mut self, dt: f32) {
        let stiffness = SPRING_FREQUENCY * SPRING_FREQUENCY;
        let damping = 2.0 * DAMPING_RATIO * SPRING_FREQUENCY;
        let steps = (dt / MAX_STEP_SECS).ceil().max(1.0) as usize;
        let step = dt / steps as f32;
        for _ in 0..steps {
            let acceleration = -stiffness * (self.value - self.target) - damping * self.velocity;
            self.velocity += acceleration * step;
            self.value += self.velocity * step;
        }
        if (self.value - self.target).abs() < SETTLE_EPSILON && self.velocity.abs() < SETTLE_EPSILON
        {
            self.value = self.target;
            self.velocity = 0.0;
        }
    }
}

/// The alert identity a banner frame belongs to. The key carries the
/// instance's activation stamp, so an alert that clears and fires again
/// opens a second time.
#[derive(Component, Clone, Debug, Default)]
pub(crate) struct BannerAlert(pub(crate) String);

/// The alert instances whose banner already opened. The alerts page remounts
/// on every folded batch; without this memory every remount would replay the
/// reveal.
#[derive(Resource, Default)]
pub(crate) struct OpenedBanners(VecDeque<String>);

impl OpenedBanners {
    /// Record the key and report whether this alert is arriving for the
    /// first time.
    fn remember(&mut self, key: &str) -> bool {
        if self.0.iter().any(|seen| seen == key) {
            return false;
        }
        self.0.push_back(key.to_owned());
        while self.0.len() > OPENED_MEMORY {
            self.0.pop_front();
        }
        true
    }
}

/// Integrate every banner spring and write the frame's height. The system
/// runs before the UI layout, so the height a banner shows in a frame is
/// always the height the spring computed for that frame.
fn advance_banners(
    time: Res<Time>,
    mut opened: ResMut<OpenedBanners>,
    mut banners: Query<(&mut BannerSpring, &mut Node, &BannerAlert)>,
) {
    let dt = time.delta_secs().clamp(0.0, MAX_STEP_SECS * 4.0);
    for (mut spring, mut node, alert) in &mut banners {
        if !spring.initialized {
            spring.initialized = true;
            if opened.remember(&alert.0) {
                spring.value = 0.0;
                spring.velocity = 0.0;
            } else {
                spring.value = spring.target;
                spring.velocity = 0.0;
            }
        } else {
            spring.advance(dt);
        }
        node.height = Val::Rem(spring.value.max(0.0));
    }
}

/// Register the banner spring: the memory resource and the integration
/// system.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<OpenedBanners>()
        .add_systems(PostUpdate, advance_banners.before(UiSystems::Prepare));
}

/// The banner's slot height in rem: two body rows plus the banner's
/// breathing room, expressed on the control ladder so the slot re-scales
/// with the reading size.
fn banner_slot_rem(palette: &UiPalette) -> f32 {
    palette.control_height_rem * 1.5
}

/// One active-alert banner: the severity plate beside the alert's headline,
/// over the firing detail, inside the frame the spring owns.
pub(crate) fn scene(
    alert: &Alert,
    headline: String,
    detail: String,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let ink = super::severity::severity_ink(alert.severity, palette);
    let icon = icon_scene(IconId::Alert, palette, ink);
    let slot = banner_slot_rem(palette);
    let key = format!(
        "{}|{}|{}",
        alert.rule_id.as_str(),
        alert.target,
        alert.active_since_ms
    );
    bsn! {
        Node {
            width: percent(100),
            height: rem(slot),
            position_type: PositionType::Relative,
            overflow: Overflow::clip_y(),
            flex_shrink: 0.0,
        }
        BackgroundColor({ palette.panel_fill })
        BannerSpring { value: 0.0, velocity: 0.0, target: slot, initialized: false }
        BannerAlert({ key })
        Children [
             Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                right: px(0.0),
                min_height: rem(slot),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(space_8()),
                padding: UiRect::horizontal(Val::Px(space_8())),
             }
             Children [
                  Node {
                    width: percent(100),
                    min_width: px(0.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                    overflow: Overflow::clip_x(),
                  }
                  Children [ @{ icon } -- Text(headline) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap } ] --
                  Text(detail) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
             ]
        ]
    }
}
