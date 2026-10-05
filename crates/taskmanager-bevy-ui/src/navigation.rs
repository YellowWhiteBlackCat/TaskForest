//! Native navigation direction and declared chrome component updates.
use crate::app::{NAV_TABS, NavTabLabelNode, NavTarget};
use crate::palette::{UiPalette, space_8};
use crate::window::chrome::AppWorkspace;
use crate::window_surface::WindowSurfaceState;
use bevy::app::App;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::{
    component::Component,
    event::Event,
    observer::On,
    query::{Or, QueryData, With},
    resource::Resource,
    system::{Commands, Local, Query, Res, ResMut},
};
use bevy::scene::{Scene, bsn, on};
use bevy::ui::{
    AlignItems, Display, FlexDirection, JustifyContent, Node, Overflow, Val, percent, px,
};
use bevy::ui_widgets::{Activate, Button};
use bevy::window::{PrimaryWindow, Window};
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::navigation::NavOrientation;
pub(crate) const RAIL_WIDTH: f32 = 64.0;
#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct ChromeNode {
    node: &'static mut Node,
    strip: Option<&'static NavigationStrip>,
    workspace: Option<&'static AppWorkspace>,
    spacer: Option<&'static NavigationSpacer>,
    label: Option<&'static NavTabLabelNode>,
    target: Option<&'static NavTarget>,
}
#[derive(Resource, Default)]
pub(crate) struct NavigationState(pub(crate) NavOrientation);
#[derive(Component, Clone, Default)]
pub(crate) struct NavigationStrip;
#[derive(Component, Clone, Default)]
pub(crate) struct NavigationSpacer;
#[derive(Component, Clone, Default)]
pub(crate) struct NavigationToggle;
#[derive(Event, Clone, Copy)]
pub(crate) struct ToggleNavigation;
fn activate(_: On<Activate>, mut commands: Commands) {
    commands.trigger(ToggleNavigation);
}
fn toggle(
    _: On<ToggleNavigation>,
    surface: Option<Res<WindowSurfaceState>>,
    mut state: ResMut<NavigationState>,
) {
    if surface.as_ref().is_some_and(|surface| surface.0.is_some()) {
        return;
    }
    state.0 = state.0.toggled();
}
pub(crate) fn button(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {Node {width:px(palette.control_height_px*1.4),height:px(palette.control_height_px*1.4),align_items:AlignItems::Center,justify_content:JustifyContent::Center} Button NavigationToggle on(activate)
    crate::tooltip::TooltipText({t("navigation.toggle").to_owned()})
    Children [@{crate::icons::icon_scene(IconId::Sidebar,18.0,palette.dim_color)}]}
}
pub(crate) fn register(app: &mut App) {
    app.init_resource::<NavigationState>().add_observer(toggle);
}
type ChromeFilter = Or<(
    With<NavigationStrip>,
    With<AppWorkspace>,
    With<NavigationSpacer>,
    With<NavTabLabelNode>,
    With<NavTarget>,
)>;

pub(crate) fn sync_layout(
    windows: Query<&Window, With<PrimaryWindow>>,
    state: Res<NavigationState>,
    mut last: Local<Option<(NavOrientation, u32)>>,
    mut nodes: Query<ChromeNode, ChromeFilter>,
) {
    let vertical = state.0 == NavOrientation::Vertical;
    let width = windows.iter().next().map_or(1180.0, Window::width);
    let key = (state.0, width.to_bits());
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    for item in &mut nodes {
        let ChromeNodeItem {
            mut node,
            strip,
            workspace,
            spacer,
            label,
            target,
        } = item;
        if workspace.is_some() {
            node.flex_direction = if vertical {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            };
        }
        if strip.is_some() {
            node.flex_direction = if vertical {
                FlexDirection::Column
            } else {
                FlexDirection::Row
            };
            node.width = if vertical {
                px(RAIL_WIDTH)
            } else {
                percent(100)
            };
            node.height = if vertical { percent(100) } else { Val::Auto };
            node.flex_shrink = 0.0;
            node.min_height = px(0.0);
            node.row_gap = px(space_8());
            node.overflow = if vertical {
                Overflow::scroll_y()
            } else {
                Overflow::scroll_x()
            };
        }
        if spacer.is_some() {
            node.display = if vertical {
                Display::None
            } else {
                Display::Flex
            };
        }
        if label.is_some() {
            node.display = if vertical || width < 800.0 {
                Display::None
            } else {
                Display::Flex
            };
        }
        if let Some(target) = target {
            node.flex_grow = if !vertical && NAV_TABS.contains(&target.0) {
                1.0
            } else {
                0.0
            };
            node.flex_shrink = if vertical { 0.0 } else { 1.0 };
            if vertical {
                node.width = percent(100);
            } else if NAV_TABS.contains(&target.0) {
                node.width = Val::Auto;
            } else {
                node.width = node.height;
            }
        }
    }
}
