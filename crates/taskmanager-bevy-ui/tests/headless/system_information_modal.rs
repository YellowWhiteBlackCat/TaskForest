//! test-intent: behavior
//!
//! System Information now consumes GPUI's grouped label/value shape: each
//! provider-owned group title and row is mounted, long values stay in bounded
//! slots, and the empty projection keeps its typed unavailable copy.

use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::scene::WorldSceneExt;
use bevy::ui::widget::Text;
use bevy::ui::{Node, OverflowAxis, Val};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::system_information::{
    SystemInformationGroup, SystemInformationRow,
};
use taskmanager_theme::{Theme, tokens::UiSize};

use super::{SystemInformationRowMarker, surface_scene, system_information_row_slots};
use crate::palette::ui_palette;
use crate::window::tests::scripted_frontend_app;

fn surface(facts: &[SystemInformationGroup]) -> Box<dyn bevy::scene::Scene> {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    surface_scene(facts, &palette)
}

fn groups() -> Vec<SystemInformationGroup> {
    vec![SystemInformationGroup {
        title_key: "system_about.kernel",
        rows: vec![SystemInformationRow {
            label_key: "system_about.release",
            value: "6.18.0-some-very-long-kernel-build-identifier".to_owned(),
        }],
    }]
}

#[test]
fn grouped_rows_mount_titles_values_and_bounded_slots() {
    let mut app = scripted_frontend_app();
    let surface = surface(&groups());
    app.world_mut()
        .spawn_scene(surface)
        .expect("the grouped review scene mounts");
    app.update();

    let world = app.world_mut();
    let texts = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(texts.contains(&t("system_about.kernel").to_owned()));
    assert!(texts.contains(&t("system_about.release").to_owned()));
    assert!(texts.contains(&"6.18.0-some-very-long-kernel-build-identifier".to_owned()));
    let rows = world
        .query_filtered::<Entity, With<SystemInformationRowMarker>>()
        .iter(world)
        .count();
    assert_eq!(rows, 1, "one mounted row per provider-owned fact");

    let slots = system_information_row_slots();
    let mut child_nodes = Vec::new();
    let mut rows_query = world.query_filtered::<&Children, With<SystemInformationRowMarker>>();
    for children in rows_query.iter(world) {
        for child in children.iter() {
            child_nodes.push(*child);
        }
    }
    assert_eq!(child_nodes.len(), 2, "label and value own separate slots");
    for child in child_nodes {
        let node = world.get::<Node>(child).expect("bounded slot node");
        assert_eq!(node.overflow.x, OverflowAxis::Clip);
        assert!(node.min_width != Val::Auto);
    }
    assert_eq!(slots.value_shrink, 0.0);
    assert!(slots.value_min_width_rem > 0.0);
}

#[test]
fn an_empty_projection_keeps_its_typed_unavailable_copy() {
    let mut app = scripted_frontend_app();
    app.world_mut()
        .spawn_scene(surface(&[]))
        .expect("the empty review scene mounts");
    app.update();
    let world = app.world_mut();
    let texts = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(texts.contains(&t("system_about.unavailable").to_owned()));
    assert_eq!(
        world
            .query_filtered::<Entity, With<SystemInformationRowMarker>>()
            .iter(world)
            .count(),
        0,
        "no fact means no invented row"
    );
}
