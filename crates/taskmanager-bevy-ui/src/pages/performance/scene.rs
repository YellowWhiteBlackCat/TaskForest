//! Scene composition for the Performance page — the `bsn!` builders that
//! turn the page's pure view-model resolvers (in the parent module) into
//! the mounted UI tree. Split from the parent to respect the per-file source
//! budget; data folding, marker vocabulary, and the refresh observer stay
//! with the parent. `content` is the page-agent entry [`crate::app`] calls.

use super::*;
use crate::palette::{UiPalette, no_wrap_text, space_4, space_8, space_12};
use crate::widgets::controls::{
    ControlTone, ControlVisual, SurfaceTone, device_row_with_accessory_scene, graph_card_scene,
    pill_scene, stat_row_scene, surface_scene,
};
use crate::widgets::layout::{
    MAIN_GRAPH_MIN_WIDTH_PX, WIDE_DEVICE_SIDEBAR_WIDTH_PX, WIDE_STATS_WIDTH_PX,
};
use bevy::color::Alpha;
use bevy::scene::{on, template_value};
use bevy::ui::prelude::{BackgroundColor, BorderRadius, FlexWrap, PositionType};
use bevy::ui_widgets::Button;

pub(super) mod blocks;
pub(super) mod chart;
pub(super) mod sidebar;

use sidebar::cpu::cpu_main_scene;
use sidebar::{device_sidebar_scene, stats_rail_scene};

/// Content-region scene for the Performance page.
pub(crate) fn content(context: &PageContext<'_>) -> impl Scene + use<> {
    let shell = context.shell;
    let palette = context.palette;
    let devices = device_sidebar_scene(shell, palette);
    let main = cpu_main_scene(shell, palette);
    let stats = stats_rail_scene(shell, palette);
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Row,
            row_gap: Val::Px(space_2()),
            column_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_2())),
            overflow: Overflow::scroll_y(),
        }
        PerformancePageRoot
        ScrollArea
        Children [
            ( { devices } ),
            ( { main } ),
            ( { stats } ),
        ]
    }
}
