//! One bounded workspace separates navigation width from page and feedback slots.
use super::{AppShellRoot, FeedbackLine, Role, SummaryLine, TextRole};
use crate::app::{ContentSlot, Page, nav_strip_scene};
use crate::palette::{UiPalette, space_8, space_12};
use bevy::ecs::{component::Component, hierarchy::Children};
use bevy::scene::{Scene, bsn};
use bevy::ui::widget::Text;
use bevy::ui::{BackgroundColor, FlexDirection, Node, UiRect, Val, percent, px};
#[derive(Component, Clone, Default)]
pub(crate) struct AppWorkspace;
pub(super) fn app_shell_scene(palette: &UiPalette, route: Page, summary: String) -> Box<dyn Scene> {
    let strip = nav_strip_scene(route, palette);
    let status: Vec<Box<dyn Scene>> = if route == Page::Performance {
        Vec::new()
    } else {
        vec![Box::new(
            bsn! {Node {width:percent(100),flex_direction:FlexDirection::Column,flex_shrink:0.0,padding:UiRect::horizontal(px(space_12()))}
            Children [Text(summary) SummaryLine TextRole(Role::Caption) -- Text("") FeedbackLine TextRole(Role::Caption)]},
        )]
    };
    Box::new(bsn! {
        Node {width:percent(100),height:percent(100),min_width:px(0.0),min_height:px(0.0),flex_direction:FlexDirection::Column} BackgroundColor({palette.window_clear}) AppShellRoot
        Children [
            Node {width:percent(100),height:percent(100),min_width:px(0.0),min_height:px(0.0),flex_direction:FlexDirection::Column,flex_grow:1.0,flex_basis:px(0.0)} AppWorkspace
            Children [@{strip} --
                Node {min_width:px(0.0),min_height:px(0.0),flex_direction:FlexDirection::Column,flex_grow:1.0,flex_basis:px(0.0)}
                Children [{status} --
                    Node {width:percent(100),min_width:px(0.0),min_height:px(0.0),flex_grow:1.0,flex_basis:px(0.0),padding:UiRect::all(Val::Px(space_8()))} BackgroundColor({palette.content_bg}) ContentSlot]
            ]
        ]
    })
}
