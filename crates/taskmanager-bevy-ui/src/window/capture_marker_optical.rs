//! Readiness facts shared by the Applications optical-card captures.

use super::capture_marker::CaptureAccess;
use super::capture_marker_interactive::has_all_texts;
use crate::glass::optical::{OpticalGlassBinding, OpticalGlassSurface, OpticalNestingDepths};
use crate::help_overlay::HelpOverlay;
use crate::menu_modal::{MenuModalChanged, MenuModalPopover};
use crate::pages::processes::card_lane::{
    ProcessAtomicCard, ProcessCardFlow, ProcessCardGroup, ProcessFocusDashboard, ProcessViewMode,
};
use crate::pages::processes::details_rail_is_admitted;
use crate::pages::processes::menu::{ProcessMenuCtx, open_for_visible_index};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res, SystemParam};
use bevy::ui::{ComputedNode, Display, Node, UiGlobalTransform};

#[derive(SystemParam)]
pub(super) struct CardLaneCapture<'w, 's> {
    mode: Option<Res<'w, ProcessViewMode>>,
    flow: Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<ProcessCardFlow>>,
    groups: Query<
        'w,
        's,
        (
            &'static ProcessCardGroup,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
        With<ProcessCardGroup>,
    >,
    dashboards: Query<
        'w,
        's,
        (
            Entity,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static Node,
        ),
        With<ProcessFocusDashboard>,
    >,
    atomic_cards: Query<
        'w,
        's,
        (
            Entity,
            &'static ProcessAtomicCard,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
        With<ProcessAtomicCard>,
    >,
}

#[derive(SystemParam)]
pub(super) struct HelpGlassCapture<'w, 's> {
    overlays: Query<'w, 's, Entity, With<HelpOverlay>>,
    surfaces: Query<'w, 's, (Entity, &'static OpticalGlassBinding), With<OpticalGlassSurface>>,
    parents: Query<'w, 's, &'static ChildOf>,
}

/// Measured facts behind a production two-layer optical capture.
///
/// The resource reports the real stack-derived depth, while the query reports
/// the live material binding: a Painted fallback on either layer cannot pass
/// as optical evidence.
#[derive(SystemParam)]
pub(super) struct NestedGlassCapture<'w, 's> {
    depths: Option<Res<'w, OpticalNestingDepths>>,
    surfaces: Query<
        'w,
        's,
        (
            Entity,
            &'static OpticalGlassBinding,
            &'static OpticalGlassSurface,
        ),
    >,
    menus: Query<'w, 's, Entity, With<MenuModalPopover>>,
    parents: Query<'w, 's, &'static ChildOf>,
}

impl NestedGlassCapture<'_, '_> {
    pub(super) fn attached_pair(&self) -> Option<(u8, u8)> {
        let depths = self.depths.as_deref()?;
        let top_level = self.surfaces.iter().any(|(entity, binding, _)| {
            matches!(binding, OpticalGlassBinding::Optical(_)) && depths.depth_of(entity) == 0
        });
        let nested_menu = self.menus.iter().any(|menu| {
            self.surfaces.iter().any(|(entity, binding, _)| {
                matches!(binding, OpticalGlassBinding::Optical(_))
                    && depths.depth_of(entity) == 1
                    && is_descendant(entity, menu, &self.parents)
            })
        });
        (top_level && nested_menu).then_some((0, 1))
    }
}

/// Whole card groups and the measured dashboard protect the lane viewport.
pub(super) fn card_lane_bounds_ready(access: &CaptureAccess<'_, '_>) -> bool {
    let lane = &access.card_lane;
    if lane
        .mode
        .as_ref()
        .is_none_or(|mode| **mode != ProcessViewMode::Cards)
    {
        return false;
    }
    let Some((flow, flow_transform)) = lane.flow.iter().next() else {
        return false;
    };
    let flow_bottom = flow_transform.translation.y + flow.size().y / 2.0;
    let flow_right = flow_transform.translation.x + flow.size().x / 2.0;
    let flow_left = flow_transform.translation.x - flow.size().x / 2.0;
    let groups: Vec<_> = lane
        .groups
        .iter()
        .filter(|(_, node, _)| node.size().x > 0.0 && node.size().y > 0.0)
        .collect();
    if !groups.is_empty() {
        return lane.dashboards.iter().any(|(_, node, _, layout)| {
            layout.display == Display::Flex && node.size().x > 0.0 && node.size().y > 0.0
        }) && groups.iter().all(|(_, node, transform)| {
            contained_in_flow(node.size(), transform, flow_left, flow_right, flow_bottom)
        });
    }
    let cards: Vec<_> = lane
        .atomic_cards
        .iter()
        .filter(|(_, _, node, _)| node.size().x > 0.0 && node.size().y > 0.0)
        .collect();
    cards.len() == 1
        && lane.dashboards.iter().all(|(_, node, _, layout)| {
            layout.display == Display::None || node.size().x == 0.0 || node.size().y == 0.0
        })
        && contained_in_flow(
            cards[0].2.size(),
            cards[0].3,
            flow_left,
            flow_right,
            flow_bottom,
        )
}

fn contained_in_flow(
    size: bevy::math::Vec2,
    transform: &bevy::ui::UiGlobalTransform,
    flow_left: f32,
    flow_right: f32,
    flow_bottom: f32,
) -> bool {
    let half = size / 2.0;
    transform.translation.x - half.x >= flow_left - 0.5
        && transform.translation.x + half.x <= flow_right + 0.5
        && transform.translation.y + half.y <= flow_bottom + 0.5
}

pub(super) fn attached_help_surface(glass: &HelpGlassCapture<'_, '_>) -> bool {
    let Some(overlay) = glass.overlays.iter().next() else {
        return false;
    };
    glass.surfaces.iter().any(|(entity, binding)| {
        matches!(binding, OpticalGlassBinding::Optical(_))
            && is_descendant(entity, overlay, &glass.parents)
    })
}

/// An optical menu capture needs an attached depth-0 surface, the mounted menu
/// attached at depth 1, and complete card-lane bounds.
pub(super) fn nested_optical_ready(
    access: &CaptureAccess<'_, '_>,
    glass: &NestedGlassCapture<'_, '_>,
) -> bool {
    glass.attached_pair() == Some((0, 1)) && card_lane_bounds_ready(access)
}

pub(super) fn optical_menu_capture_ready(access: &mut CaptureAccess<'_, '_>) -> bool {
    if !card_lane_bounds_ready(access) {
        return false;
    }
    if !access.state.menu_open_requested {
        let anchor = access
            .card_lane
            .atomic_cards
            .iter()
            .find(|(_, _, node, _)| node.size().x > 0.0 && node.size().y > 0.0)
            .map(|(entity, card, _, _)| (entity, card.0))
            .or_else(|| {
                access
                    .card_lane
                    .dashboards
                    .iter()
                    .find(|(_, node, _, layout)| {
                        layout.display == Display::Flex
                            && node.size().x > 0.0
                            && node.size().y > 0.0
                    })
                    .map(|(entity, _, _, _)| (entity, access.track.shell.selected))
            });
        let Some((anchor, index)) = anchor else {
            return false;
        };
        let Some(menu) = access.process_menu.as_mut() else {
            return false;
        };
        menu.set_trigger(anchor);
        if !open_for_visible_index(menu, &access.track.shell, index) {
            return false;
        }
        access.commands.trigger(MenuModalChanged::<ProcessMenuCtx>(
            true,
            std::marker::PhantomData,
        ));
        access.state.menu_open_requested = true;
        return false;
    }
    if !super::capture_marker_interactive::process_menu_mounted(&access.interactive) {
        return false;
    }
    !hardware_optical_capture_requested()
        || (nested_optical_ready(access, &access.nested_glass)
            && access.chain_cameras.iter().any(|camera| camera.is_active))
}

fn hardware_optical_capture_requested() -> bool {
    std::env::var("TM_BEVY_CAPTURE_HARDWARE_WESL").is_ok_and(|value| value == "1")
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
