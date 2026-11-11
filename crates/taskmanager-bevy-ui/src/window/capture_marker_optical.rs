//! Readiness facts shared by the Applications optical-card captures.

use super::capture_marker::CaptureAccess;
use super::capture_marker_interactive::has_all_texts;
use crate::glass::optical::{OpticalGlassBinding, OpticalGlassSurface};
use crate::help_overlay::HelpOverlay;
use crate::pages::processes::card_lane::ProcessViewMode;
use crate::pages::processes::details_rail_is_admitted;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, SystemParam};
use bevy::ui::Display;

#[derive(SystemParam)]
pub(super) struct HelpGlassCapture<'w, 's> {
    overlays: Query<'w, 's, Entity, With<HelpOverlay>>,
    surfaces: Query<'w, 's, (Entity, &'static OpticalGlassBinding), With<OpticalGlassSurface>>,
    parents: Query<'w, 's, &'static ChildOf>,
}

/// Whole card groups and the measured dashboard protect the lane viewport.
pub(super) fn card_lane_bounds_ready(access: &CaptureAccess<'_, '_>) -> bool {
    if access
        .card_mode
        .as_ref()
        .is_none_or(|mode| **mode != ProcessViewMode::Cards)
        || !access
            .dashboards
            .iter()
            .any(|(node, _)| node.size().x > 0.0 && node.size().y > 0.0)
        || !access
            .card_groups
            .iter()
            .any(|(_, node, _)| node.size().x > 0.0 && node.size().y > 0.0)
    {
        return false;
    }
    let Some((flow, flow_transform)) = access.card_flow.iter().next() else {
        return false;
    };
    let flow_bottom = flow_transform.translation.y + flow.size().y / 2.0;
    let flow_right = flow_transform.translation.x + flow.size().x / 2.0;
    access.card_groups.iter().all(|(_, node, transform)| {
        let half = node.size() / 2.0;
        transform.translation.x - half.x >= flow_transform.translation.x - flow.size().x / 2.0 - 0.5
            && transform.translation.x + half.x <= flow_right + 0.5
            && transform.translation.y + half.y <= flow_bottom + 0.5
    })
}

pub(super) fn attached_help_surface(glass: &HelpGlassCapture<'_, '_>) -> bool {
    let Some(overlay) = glass.overlays.iter().next() else {
        return false;
    };
    glass.surfaces.iter().any(|(entity, binding)| {
        matches!(binding, OpticalGlassBinding::Attached(_))
            && is_descendant(entity, overlay, &glass.parents)
    })
}

pub(super) fn help_capture_ready(
    access: &CaptureAccess<'_, '_>,
    glass: &HelpGlassCapture<'_, '_>,
    require_optical: bool,
) -> bool {
    access.track.shell.help_open()
        && glass.overlays.iter().count() == 1
        && has_all_texts(
            &access.text,
            &[
                "settings.keyboard",
                "settings.keys_pages",
                "common.all",
                "common.close",
            ],
        )
        && (!require_optical || attached_help_surface(glass))
}

fn is_descendant(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    let mut current = entity;
    for _ in 0..64 {
        if current == ancestor {
            return true;
        }
        let Ok(parent) = parents.get(current) else {
            return false;
        };
        current = parent.parent();
    }
    false
}

/// The whole details rail is admitted or hidden as one measured group.
///
/// `COMPACT_CONTENT_WIDTH` comes from the shared presentation vocabulary;
/// this helper only compares the measured `Display` verdict against the
/// width band the targeted capture names.
pub(super) fn details_rail_capture_ready(
    scenario: Option<&'static str>,
    access: &CaptureAccess<'_, '_>,
) -> bool {
    let expected = match scenario {
        Some("applications-card-lane-details-wide") => Display::Flex,
        Some("applications-card-lane-details-compact") => Display::None,
        _ => return false,
    };
    let Some(container) = access.details.iter().next() else {
        return false;
    };
    let Ok(page) = access.process_pages.single() else {
        return false;
    };
    let measured_width = page.size().x * page.inverse_scale_factor();
    container.display == expected
        && details_rail_is_admitted(measured_width) == (expected == Display::Flex)
}
