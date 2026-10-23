//! The hierarchy guide: the connectors that show which ancestor owns a row.
//!
//! Every nesting level of the tree gets one guide column. The column's
//! vertical hairline runs through the rows that share one ancestor, and the
//! deepest column carries the row's own elbow — the short horizontal bar that
//! connects the row to its parent. The guide replaces the old flat indent:
//! the total indent is unchanged, but the ownership is now visible.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{AlignSelf, BackgroundColor, Node, PositionType, Val, percent, px};

use crate::palette::{UiPalette, space_8};

use super::plan::{GuideColumn, GuideStroke};

/// The hairline width of a guide connector, in logical pixels.
///
/// A hairline is real geometry, not a layout length: it stays a pixel so the
/// connector reads as one crisp rule at every reading size.
const GUIDE_HAIRLINE_PX: f32 = 1.0;

/// Marker naming the guide column a node draws.
///
/// The column index is the nesting level it belongs to; `elbow` says the
/// column carries the row's own elbow rather than an inherited connector.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TreeGuideColumn {
    /// The nesting level of this column.
    pub(crate) index: usize,
    /// The column carries the row's own elbow.
    pub(crate) elbow: bool,
}

/// The guide scene of one row: one column per nesting level.
pub(super) fn guide_scene(columns: &[GuideColumn], palette: &UiPalette) -> Vec<Box<dyn Scene>> {
    let ink = palette.border_color;
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let mut parts: Vec<Box<dyn Scene>> = Vec::new();
            if column.stroke != GuideStroke::None {
                parts.push(stroke_scene(column.stroke, ink));
            }
            if column.elbow {
                parts.push(elbow_scene(ink));
            }
            let elbow = column.elbow;
            Box::new(bsn! {
                Node {
                    width: px(space_8()),
                    height: percent(100.0),
                    flex_shrink: 0.0,
                    align_self: AlignSelf::Stretch,
                }
                TreeGuideColumn {
                    index: { index },
                    elbow: { elbow },
                }
                Pickable::IGNORE
                Children [{ parts }]
            }) as Box<dyn Scene>
        })
        .collect()
}

/// The vertical connector of one guide column.
fn stroke_scene(stroke: GuideStroke, ink: Color) -> Box<dyn Scene> {
    let height: Val = match stroke {
        GuideStroke::Half => percent(50.0),
        GuideStroke::Full | GuideStroke::None => percent(100.0),
    };
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            width: px(GUIDE_HAIRLINE_PX),
            height: { height },
        }
        BackgroundColor({ ink })
        Pickable::IGNORE
    })
}

/// The elbow that connects the row to its parent.
fn elbow_scene(ink: Color) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: percent(50.0),
            width: percent(100.0),
            height: px(GUIDE_HAIRLINE_PX),
        }
        BackgroundColor({ ink })
        Pickable::IGNORE
    })
}
