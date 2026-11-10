//! Capture readiness for the Performance page's per-core grid reveal.

use crate::pages::performance::scene::sidebar::cores::{CoreGridContent, CoreGridReveal};
use crate::pages::performance::{DeviceCategoryKind, DeviceViewCategory};
use crate::widgets::layout_budget::PerformanceLowerBand;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, SystemParam};
use bevy::ui::{ComputedNode, Display, Node};

/// The four measured surfaces the Cores card's readiness is stated over: the
/// two typed group admissions that decide whether the card owns any pixels,
/// and the reveal body with the grid content it clips.
#[derive(SystemParam)]
pub(super) struct CoreGridCaptureAccess<'w, 's> {
    lower_bands: Query<'w, 's, &'static Node, With<PerformanceLowerBand>>,
    category_bands: Query<'w, 's, (&'static DeviceViewCategory, &'static Node)>,
    bodies: Query<'w, 's, (&'static ComputedNode, &'static Children), With<CoreGridReveal>>,
    contents: Query<'w, 's, &'static ComputedNode, With<CoreGridContent>>,
}

/// The Cores card readiness. The card is admitted as a WHOLE group by typed
/// display — the shared rung hides the lower band in a compact window, and
/// another device's detail view hides the CPU category — so an unadmitted
/// card owns no pixels and gates nothing. Reading the display (never a
/// measured size) keeps an admitted-but-not-yet-laid-out frame from reading
/// as "unadmitted" and releasing an unpainted grid.
///
/// Once the card is admitted its reveal body must already show the whole
/// measured grid: a body shorter than the grid it clips is still easing
/// toward its open height, and a body that never measured the grid clips it
/// to nothing. The painted geometry, not an elapsed clock, proves the frame.
pub(super) fn core_grid_ready(access: &CoreGridCaptureAccess<'_, '_>) -> bool {
    let band_admitted = access
        .lower_bands
        .iter()
        .any(|node| node.display != Display::None);
    let cpu_admitted = access.category_bands.iter().any(|(category, node)| {
        category.0 == DeviceCategoryKind::Cpu && node.display != Display::None
    });
    if !band_admitted || !cpu_admitted {
        return true;
    }
    access.bodies.iter().all(|(body, children)| {
        let Some(content) = children.first() else {
            return false;
        };
        access.contents.get(*content).is_ok_and(|computed| {
            computed.size().y > 0.0 && body.size().y + 0.5 >= computed.size().y
        })
    })
}
