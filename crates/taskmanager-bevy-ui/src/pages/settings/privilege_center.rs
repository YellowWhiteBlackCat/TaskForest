//! Privilege helper management section for Bevy Settings page.

use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;
use taskmanager_application::i18n::t;

use crate::palette::{UiPalette, space_2, space_4, space_8};
use crate::window::{Role, TextRole};

pub(crate) fn privileges_section_scene(palette: &UiPalette) -> Box<dyn Scene> {
    let rows: Vec<Box<dyn Scene>> = vec![
        Box::new(privilege_row_scene(
            t("gpu.per_engine_title"),
            t("settings.privileges_enabled"),
            palette,
        )),
        Box::new(privilege_row_scene(
            t("system.memory_inventory"),
            t("settings.privileges_enabled"),
            palette,
        )),
        Box::new(privilege_row_scene(
            t("cpu.package_power"),
            t("settings.privileges_authorize_hint"),
            palette,
        )),
        Box::new(privilege_row_scene(
            t("cpu.msr_readouts"),
            t("settings.privileges_authorize_hint"),
            palette,
        )),
    ];

    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_4()),
            margin: UiRect::top(Val::Px(space_8())),
        }
        Children [
            ( Text(t("settings.privileges")) TextRole(Role::Heading) ),
            ( Text(t("settings.privileges_hint")) TextRole(Role::Caption) ),
            { rows },
        ]
    })
}

fn privilege_row_scene(
    label: &'static str,
    status: &'static str,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let line = format!("{label}: {status}");
    bsn! {
        Node {
            width: percent(100),
            min_height: px(palette.control_height_px),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_2())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.content_bg })
        Children [
            ( Text(line) TextRole(Role::Body) ),
        ]
    }
}
