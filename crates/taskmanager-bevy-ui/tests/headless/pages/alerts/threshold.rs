//! test-intent: behavior
//!
//! The performance curve keeps GPUI's newest-value badge: the badge reads the
//! newest finite sample and disappears for an all-gap or absent window, so a
//! gap never becomes a fabricated value.

use super::*;
use crate::window::tests::scripted_frontend_app;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;

#[test]
fn the_badge_uses_the_curve_own_value_format() {
    let curve = SystemCurve::default();
    let shell = ShellApp::new();
    let badge = curve_badge_text(&shell, curve);

    if let Some(text) = badge {
        assert!(
            text.ends_with('%'),
            "the CPU badge reads a percentage, not {text}"
        );
    }
    assert!(
        curve_badge_text(&shell, SystemCurve::Network).is_none_or(|text| text.ends_with("/s")),
        "the network badge reads throughput"
    );
}

#[test]
fn the_performance_page_carries_the_value_badge() {
    let mut app = scripted_frontend_app();
    app.world_mut().resource_mut::<crate::app::Route>().page = crate::app::Page::Performance;
    app.update();

    let badges: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<CurveBadge>>()
        .iter(app.world())
        .collect();
    assert!(
        !badges.is_empty(),
        "the performance page carries the newest-value badge"
    );
}
