//! Bevy's binding of the shared Performance layout budget.
//!
//! The frame's measured page slot is classified once through the neutral
//! `presentation::budget` fold. Scene builders receive the resulting typed
//! verdict; they never re-read window pixels or invent a second rung ladder.

use bevy::ecs::component::Component;
use bevy::ecs::query::{Has, Or, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::ui::prelude::ComputedNode;
use bevy::ui::{Display, Node};
use taskmanager_ui_contract::FrontendShape;
use taskmanager_ui_contract::presentation::budget::{
    ContentSlot, PageLayoutBudget, PerformancePageBudget, PerformanceSlotRequest,
};
use taskmanager_ui_contract::presentation::thresholds::bands;
use taskmanager_ui_contract::presentation::vocabulary::{
    PerformanceChartInventory, PerformanceVerticalRunway, VerticalSpace,
};

/// One frame's immutable budget, resolved from the shape's registered bands
/// and the measured page slot. A page that consumes this value reads a typed
/// verdict, never a raw pixel height.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct PerformanceFrameBudget {
    pub(crate) page: PageLayoutBudget,
    pub(crate) performance: PerformancePageBudget,
}

/// The one query that reads both whole-group markers. A named alias keeps the
/// admission system's parameter list below Clippy's type-complexity ceiling
/// while the `Has` pair still names the two independent group decisions.
type GroupQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Node,
        Has<PerformanceLowerBand>,
        Has<PerformanceVerticalGroup>,
    ),
    Or<(With<PerformanceLowerBand>, With<PerformanceVerticalGroup>)>,
>;

/// The one root whose measured page slot feeds the shared classifier. The
/// window scene mounts it; the system below reads its `ComputedNode` after
/// the engine's layout pass, so no page reads raw window pixels.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceBudgetSlot;

/// The optional lower band is one group: its marker node is shown or hidden
/// wholesale from the shared runway verdict, never clipped into a half row.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceLowerBand;

/// The memory composition legend is one whole group. It is admitted from the
/// shared vertical-capacity bucket instead of a window-height constant.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PerformanceVerticalGroup;

/// Resolve the current page slot into the shared bands once per frame.
/// Apply the current frame's typed verdict to whole lower-band groups. The
/// verdict never paints a half group: each marker is shown or hidden as one
/// node.
pub(crate) fn sync_performance_frame_budget(
    budget: Res<PerformanceFrameBudget>,
    mut groups: GroupQuery<'_, '_>,
) {
    let lower_display = if budget.admits_full_inventory() {
        Display::Flex
    } else {
        Display::None
    };
    let group_display = if budget.page.vertical_space == VerticalSpace::Generous {
        Display::Flex
    } else {
        Display::None
    };
    for (mut node, lower_band, vertical_group) in &mut groups {
        let display = if lower_band {
            lower_display
        } else if vertical_group {
            group_display
        } else {
            continue;
        };
        if node.display != display {
            node.display = display;
        }
    }
}

/// Resolve the current page slot into the shared bands once per frame.
///
/// The engine's layout runs in `PostUpdate` after this chain, so this reads
/// the previous frame's settled `ComputedNode`; a resize reclassifies on the
/// next frame just like the toolkit-native layouts do. With no measured slot
/// the default is the Floor rung: no optional band is fabricated as visible.
pub(crate) fn sync_performance_frame_resolution(
    slots: Query<&ComputedNode, With<PerformanceBudgetSlot>>,
    mut budget: ResMut<PerformanceFrameBudget>,
) {
    let measured = slots.iter().next();
    let width = measured
        .map(|node| node.size().x * node.inverse_scale_factor)
        .unwrap_or(0.0);
    let height = measured
        .map(|node| node.size().y * node.inverse_scale_factor)
        .unwrap_or(0.0);
    let next = PerformanceFrameBudget::for_page_slot(width, height);
    if *budget != next {
        *budget = next;
    }
}

/// The cold-start verdict: no measured slot means no optional lower band.
impl Default for PerformanceFrameBudget {
    fn default() -> Self {
        Self::for_page_slot(0.0, 0.0)
    }
}

impl PerformanceFrameBudget {
    /// Resolve one measured page slot through the shared folds. The content
    /// height drives the same runway and chart-inventory rules the other three
    /// shapes spend; the lower band is a whole-group admission, not a clip.
    #[must_use]
    pub(crate) fn for_page_slot(slot_width: f32, slot_height: f32) -> Self {
        let shape_bands = bands(FrontendShape::Bevy);
        let slot = ContentSlot::for_body(slot_width, slot_height, &shape_bands);
        let page = PageLayoutBudget::from_slot(slot, slot.profile.navigation_default(), None);
        let performance = PerformancePageBudget::allocate(
            PerformanceSlotRequest {
                profile: slot.profile,
                workspace_width: slot.width + slot.page_padding,
                content_height: slot.height,
                page_padding: slot.page_padding,
                sidebar_visible: true,
                persisted_sidebar_width: None,
            },
            &shape_bands,
        );
        Self { page, performance }
    }

    /// Whether the whole chart inventory composes. The product bit folds both
    /// axes; a narrow slot must not admit secondary charts either.
    #[must_use]
    pub(crate) fn admits_full_inventory(self) -> bool {
        matches!(
            self.performance.chart_inventory,
            PerformanceChartInventory::Full
        ) && self.performance.vertical == PerformanceVerticalRunway::Charts
    }
}

#[cfg(test)]
#[path = "../../tests/headless/layout_budget.rs"]
mod tests;
