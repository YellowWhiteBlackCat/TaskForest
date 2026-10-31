//! The battery icon and its charge flow (BEVY-164).
//!
//! The icon is the battery's OWN mark, drawn from the theme's tokens: the
//! body, the terminal nub, and a fill that carries the observed charge
//! percentage. While the provider reports the pack as **charging**, a
//! travelling band rides the fill — the flow is the observed charge state
//! made visible, not a timer that runs whether or not the pack charges.
//!
//! The motion is a typed per-frame progress: the band's phase is a component
//! field advanced by one system, never a sleep and never a CSS-style
//! transition the engine does not run.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res};
use bevy::scene::{Scene, bsn};
use bevy::time::Time;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderColor, BorderRadius, FlexDirection, Node, Overflow,
    PositionType, UiRect, Val, percent, px, rem,
};
use taskforest_core::core::power::BatteryInfo;

use crate::pages::performance::metrics::battery::BatteryChargeStates;
use crate::palette::{UiPalette, space_1};

/// How long the band takes to travel the fill, in seconds.
const FLOW_SECONDS: f32 = 1.8;

/// The battery icon's charge flow.
///
/// The flow belongs to ONE battery: the system resolves the observed charge
/// state of that battery from the folded projection, so the icon flows while
/// the pack charges and stands still while it does not.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub(crate) struct BatteryChargeFlow {
    /// The battery this flow belongs to.
    pub(crate) battery: String,
    /// The band's position along the fill, as a fraction of the fill's width.
    pub(crate) phase: f32,
    /// Whether the observed charge state asks for the flow.
    pub(crate) charging: bool,
}

/// The battery icon: the body, the terminal nub, the charge fill, and — while
/// the pack charges — the travelling flow band.
///
/// The proportions ride the rem axis with the rest of the icon ladder, so the
/// mark re-scales with the reader's UI-size choice.
///
/// `charge_pct` is the FOLDED observation: the data layer owns the read
/// (ARCH.md §8.1), so the scene paints a fact instead of folding one.
pub(crate) fn battery_icon_scene(
    battery: &BatteryInfo,
    charge_pct: Option<u8>,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let extent = palette.icon_rem;
    let body_width = extent * 0.72;
    let body_height = extent * 0.44;
    let nub_width = extent * 0.08;
    let nub_height = extent * 0.2;
    let radius = extent * 0.08;
    let band_width = extent * 0.22;
    let charge = charge_pct.unwrap_or(0);
    let fill = f32::from(charge).clamp(0.0, 100.0);
    let id = battery.id.clone();
    let outline = palette.dim_color;
    let fill_ink = palette.accent;
    let flow_ink = palette.content_bg;
    Box::new(bsn! {
        Node {
            width: rem(extent),
            height: rem(extent),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        Children [
            Node {
                width: rem(body_width),
                height: rem(body_height),
                border: UiRect::all(px(space_1())),
                border_radius: BorderRadius::all(rem(radius)),
                overflow: Overflow::clip(),
                position_type: PositionType::Relative,
            }
            BorderColor { top: { outline }, right: { outline }, bottom: { outline }, left: { outline } }
            Children [
                // The fill carries the OBSERVED charge percentage; an
                // unobserved pack fills nothing rather than a made-up share.
                Node {
                    width: percent(fill),
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                    bottom: px(0.0),
                    overflow: Overflow::clip(),
                }
                BackgroundColor({ fill_ink })
                Children [
                    // The travelling band: the charge flow, clipped to the
                    // filled part of the pack.
                    Node {
                        width: rem(band_width),
                        position_type: PositionType::Absolute,
                        left: percent(0.0),
                        top: px(0.0),
                        bottom: px(0.0),
                    }
                    BackgroundColor({ flow_ink })
                    BatteryChargeFlow { battery: { id.clone() }, phase: 0.0, charging: false }
                ]
            ]
            --
            Node {
                width: rem(nub_width),
                height: rem(nub_height),
                border_radius: BorderRadius::all(rem(radius)),
            }
            BackgroundColor({ outline })
        ]
    })
}

/// Advance the charge flow: the band travels the fill while the observed
/// charge state says the pack is charging.
///
/// The state comes from the folded projection, not from a timer: a pack that
/// stops charging stops flowing on the next frame.
fn advance_charge_flow(
    time: Res<Time>,
    charge_states: Res<BatteryChargeStates>,
    mut flows: Query<(&mut BatteryChargeFlow, &mut Node)>,
) {
    let delta = time.delta_secs();
    for (mut flow, mut node) in &mut flows {
        let charging = charge_states.is_charging(&flow.battery);
        flow.charging = charging;
        if !charging {
            continue;
        }
        flow.phase = (flow.phase + delta / FLOW_SECONDS) % 1.0;
        let left = Val::Percent(flow.phase * 100.0);
        if node.left != left {
            node.left = left;
        }
    }
}

/// Register the charge flow's system.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_systems(
        bevy::app::PostUpdate,
        advance_charge_flow.after(bevy::ui::UiSystems::PostLayout),
    );
}
