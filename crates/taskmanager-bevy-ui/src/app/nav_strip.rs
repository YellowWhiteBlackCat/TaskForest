//! The product navigation strip scene.

use bevy::ecs::hierarchy::Children;
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn, on};
use bevy::text::LineBreak;
use bevy::text::TextColor;
use bevy::text::TextLayout;
use bevy::ui::BorderColor;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, BoxShadow, FlexDirection, JustifyContent, Node,
    Overflow, UiRect, Val, percent, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{ControlOrientation, ScrollArea, TabActivation, TabList};
use taskmanager_theme::GlassSurfaceRole;
use taskmanager_ui_contract::IconId;

use super::{NAV_TABS, Page, nav_strip_ready, nav_tab_scene, nav_trailing_scene};
use crate::navigation::{NavigationSpacer, NavigationStrip};
use crate::palette::space_4;
use crate::palette::{UiPalette, space_8};
use crate::window::{Role, TextRole};

/// Shared tabs and trailing route affordances on one glass navigation band.
pub(crate) fn nav_strip_scene(route: Page, palette: &UiPalette) -> impl Scene + use<> {
    let nav_gap_rem = palette.rem_ratio(space_8());
    let nav_padding_rem = palette.rem_ratio(space_8());
    let radius_rem = palette.panel_radius_rem();
    let nav_fill = palette.nav_bg;
    let nav_edge = palette.glass_sidebar_edge;
    let role = GlassSurfaceRole::Chrome;
    let tier = role.tier();
    let shadow = crate::glass::surface_shadow(role, palette);
    let tabs: Vec<Box<dyn Scene>> = NAV_TABS
        .iter()
        .map(|&page| Box::new(nav_tab_scene(page, page == route, palette)) as Box<dyn Scene>)
        .collect();
    // The strip's right edge carries only the affordances with no tab slot.
    let trailing: Vec<Box<dyn Scene>> = [Page::Alerts, Page::Settings]
        .iter()
        .map(|&page| Box::new(nav_trailing_scene(page, page == route, palette)) as Box<dyn Scene>)
        .collect();
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Rem(nav_gap_rem),
            padding: UiRect::all(Val::Rem(nav_padding_rem)),
            border: UiRect::all(Val::Rem(tier.edge_width_rem())),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
        }
        BackgroundColor({ nav_fill })
        BorderColor::all(nav_edge)
        BoxShadow({ shadow })
        NavigationStrip
        // The headless tab-list widget: roving arrow-key focus, the shared
        // tablist/tab AT-SPI roles, and `ValueChange` selection events. The
        // route resource stays the sole selection authority.
        TabList {
            orientation: ControlOrientation::Horizontal,
            activation: TabActivation::Manual,
        }
        on(nav_strip_ready)
        ScrollArea
        Children [
            { tabs } --
            Node { flex_grow: 1.0 } NavigationSpacer --
            @{crate::navigation::button(palette)} --
            { trailing }
        ]
    }
}

pub(crate) fn demo_notice_scene(notice: String, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::FlexStart,
            padding: UiRect::axes(
                Val::Px(space_8()),
                Val::Px(space_4()),
            ),
        }
        Children [
            Node {
                padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())),
                min_height: rem(palette.control_height_rem * 0.8),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_4()),
                border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                overflow: Overflow::clip_x(),
            }
            BackgroundColor({ palette.nav_active_bg })
            Children [
                @{crate::icons::icon_scene(
                    IconId::Health,
                    palette,
                    palette.success_color,
                )}
                Pickable::IGNORE --
                Text(notice)
                TextRole(Role::Caption)
                TextColor({ palette.success_color })
                TextLayout { linebreak: LineBreak::NoWrap }
            ]
        ]
    })
}
