//! test-intent: behavior
//!
//! The rule editor's threshold is a real slider: the widget is mounted with
//! the rule's threshold and the metric's own range, and the typed value
//! change a drag emits becomes the canonical rule edit.

use super::tests::{headless_shell_app, mount_alerts};
use super::*;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ui_widgets::{SetSliderValue, Slider, SliderRange, SliderValue, SliderValueChange};

fn cpu_rule_threshold(app: &mut App) -> f32 {
    app.world()
        .non_send::<FrontendTrack>()
        .shell
        .projection()
        .alert_center
        .managed_rules()
        .iter()
        .find(|managed| managed.rule.id.as_str() == "cpu-high")
        .expect("the default cpu rule")
        .rule
        .threshold
}

fn cpu_slider(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<(Entity, &editor::ThresholdSlider), With<editor::ThresholdSlider>>()
        .iter(app.world())
        .find(|(_, target)| {
            target
                .0
                .as_ref()
                .is_some_and(|id| id.as_str() == "cpu-high")
        })
        .map(|(entity, _)| entity)
        .expect("the cpu rule's threshold slider")
}

#[test]
fn the_threshold_slider_is_mounted_with_the_rule_value_and_range() {
    let mut app = headless_shell_app();
    mount_alerts(&mut app);
    let sliders: Vec<(f32, f32, f32)> = app
        .world_mut()
        .query_filtered::<(&SliderValue, &SliderRange), With<Slider>>()
        .iter(app.world())
        .map(|(value, range)| (value.0, range.start(), range.end()))
        .collect();
    assert!(
        !sliders.is_empty(),
        "the rule editor mounts a threshold slider per rule"
    );
    let cpu = sliders
        .iter()
        .find(|(value, _, _)| (*value - 90.0).abs() < f32::EPSILON)
        .expect("the cpu rule's slider carries the rule's threshold");
    assert_eq!(cpu.1, 0.0, "the percentage metric's range starts at zero");
    assert_eq!(
        cpu.2, 100.0,
        "the percentage metric's range ends at one hundred"
    );
}

#[test]
fn dragging_the_threshold_slider_applies_the_canonical_rule_edit() {
    let mut app = headless_shell_app();
    app.add_plugins(bevy::ui_widgets::SliderPlugin);
    mount_alerts(&mut app);
    let slider = cpu_slider(&mut app);

    // The widget's own value path: the typed event a drag emits.
    app.world_mut().trigger(SetSliderValue {
        entity: slider,
        change: SliderValueChange::Absolute(95.0),
    });
    // The deferred event reaches the page observer, and the finished
    // interaction remounts the page with the fresh projection.
    app.update();
    app.update();

    assert_eq!(
        cpu_rule_threshold(&mut app),
        95.0,
        "the drag reached the canonical rule through the shared edit entry"
    );
    let remounted = cpu_slider(&mut app);
    assert_eq!(
        app.world()
            .get::<SliderValue>(remounted)
            .expect("the remounted slider keeps its own value")
            .0,
        95.0,
        "the widget's value follows the drag"
    );
}
