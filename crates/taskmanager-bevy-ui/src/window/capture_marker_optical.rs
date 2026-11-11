//! Readiness facts shared by the Applications optical-card captures.

use super::capture_marker::CaptureAccess;
use crate::pages::processes::card_lane::ProcessViewMode;
use crate::pages::processes::details_rail_is_admitted;
use bevy::ui::Display;

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
