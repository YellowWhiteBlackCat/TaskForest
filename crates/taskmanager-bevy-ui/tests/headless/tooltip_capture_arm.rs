//! test-intent: behavior
//!
//! The capture arm must publish the tooltip's real state machine session, not
//! a fixture-only card. This proof feeds measured `TooltipText` triggers into
//! the pure anchor admission and asserts the Applications "Choose columns"
//! control wins, so a capture of the optical hero anchors to that control
//! instead of the nav toggle. It also asserts the measured-only fallback.

use bevy::ecs::entity::Entity;
use bevy::math::Vec2;
use taskmanager_application::i18n::{Language, set_language, t};

use super::capture_target_from_measured;

fn trigger(index: u32, label: String, size: Vec2) -> (Entity, String, Vec2) {
    (
        Entity::from_raw_u32(index).expect("a valid raw entity index"),
        label,
        size,
    )
}

#[test]
fn the_tooltip_capture_arm_anchors_the_named_applications_trigger() {
    set_language(Language::En);
    let nav = trigger(1, t("navigation.toggle").to_owned(), Vec2::new(100.0, 24.0));
    let named_label = t("proc.choose_columns").to_owned();
    let named = trigger(2, named_label.clone(), Vec2::new(100.0, 24.0));
    let targets = vec![nav.clone(), named.clone()];

    assert_eq!(
        capture_target_from_measured(&targets, &named_label),
        Some((named.0, named_label)),
        "the capture anchors the Applications trigger, not the first measured one"
    );
    assert_eq!(
        capture_target_from_measured(&targets, "a missing label"),
        Some((nav.0, nav.1)),
        "an unnamed scenario keeps the first measured trigger"
    );
}

#[test]
fn an_unmeasured_named_trigger_never_anchors_the_card() {
    set_language(Language::En);
    let named_label = t("proc.choose_columns").to_owned();
    let hidden = trigger(3, named_label.clone(), Vec2::ZERO);
    let visible = trigger(4, t("navigation.toggle").to_owned(), Vec2::new(100.0, 24.0));
    let targets = vec![hidden, visible.clone()];

    assert_eq!(
        capture_target_from_measured(&targets, &named_label),
        Some((visible.0, visible.1)),
        "an unmeasured named trigger cannot place the card"
    );
    assert_eq!(
        capture_target_from_measured(&[], &named_label),
        None,
        "no measured trigger publishes no session"
    );
}
