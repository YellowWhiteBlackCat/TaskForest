//! The sliding tab indicator of the properties modal.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut, SystemParam};
use bevy::ui::prelude::{ComputedNode, Node, UiGlobalTransform, Val};

use super::{
    ProcessPropertiesPresentation, ProcessPropertiesSection, ProcessPropertiesTab, PropertiesPaint,
};

/// The height of the sliding indicator, in px.
///
/// Modal geometry is the documented pixel exception to the rem sizing axis: an
/// indicator rule is a hairline in screen space, not a type-ladder step.
pub(crate) const TAB_INDICATOR_HEIGHT_PX: f32 = 2.0;

/// The share of the slide completed per frame.
///
/// The slide is a typed state, not a wall-clock animation: the progress
/// advances one step per rendered frame, so the indicator always lands
/// exactly on the target tab.
const TAB_INDICATOR_STEP: f32 = 0.125;

/// The sliding indicator of the properties tab strip.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TabIndicator;

/// Where the sliding indicator stands when a surface is mounted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct IndicatorPlacement {
    /// The horizontal offset of the indicator from the strip's left edge, in px.
    pub(crate) x: f32,
    /// The width of the indicator, in px.
    pub(crate) width: f32,
}

/// The typed animation state of the sliding tab indicator.
///
/// The properties modal rebuilds its whole surface when the section changes,
/// so the slide state cannot live on the indicator entity: it lives in this
/// resource, which survives the rebuild and lets the freshly mounted
/// indicator start exactly where the previous one stood.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct TabIndicatorMotion {
    /// The section the indicator is sliding toward.
    pub(crate) target: Option<ProcessPropertiesSection>,
    /// The horizontal offset the slide starts from, in px.
    pub(crate) from_x: f32,
    /// The horizontal offset the slide ends at, in px.
    pub(crate) to_x: f32,
    /// The width the slide starts from, in px.
    pub(crate) from_width: f32,
    /// The width the slide ends at, in px.
    pub(crate) to_width: f32,
    /// The slide progress: 0.0 at the start, 1.0 once it landed.
    pub(crate) progress: f32,
}

impl Default for TabIndicatorMotion {
    fn default() -> Self {
        Self {
            target: None,
            from_x: 0.0,
            to_x: 0.0,
            from_width: 0.0,
            to_width: 0.0,
            progress: 0.0,
        }
    }
}

impl TabIndicatorMotion {
    /// The eased slide progress, in 0.0..=1.0.
    fn eased(&self) -> f32 {
        let progress = self.progress.clamp(0.0, 1.0);
        progress * progress * (3.0 - 2.0 * progress)
    }

    /// The horizontal offset the indicator currently stands at, in px.
    pub(crate) fn current_x(&self) -> f32 {
        self.from_x + (self.to_x - self.from_x) * self.eased()
    }

    /// The width the indicator currently has, in px.
    pub(crate) fn current_width(&self) -> f32 {
        self.from_width + (self.to_width - self.from_width) * self.eased()
    }

    /// Where the indicator stands when a surface is mounted.
    pub(crate) fn placement(&self) -> IndicatorPlacement {
        IndicatorPlacement {
            x: self.current_x(),
            width: self.current_width(),
        }
    }
}

/// The typed access the sliding indicator reads its geometry from.
#[derive(SystemParam)]
pub(super) struct TabIndicatorAccess<'w, 's> {
    paint: Res<'w, PropertiesPaint>,
    state: Res<'w, ProcessPropertiesPresentation>,
    motion: ResMut<'w, TabIndicatorMotion>,
    children: Query<'w, 's, &'static Children>,
    parents: Query<'w, 's, &'static ChildOf>,
    indicators: Query<'w, 's, &'static mut Node, With<TabIndicator>>,
    nodes: Query<'w, 's, &'static ComputedNode>,
    transforms: Query<'w, 's, &'static UiGlobalTransform>,
    tabs: Query<
        'w,
        's,
        (
            &'static ProcessPropertiesTab,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
    >,
}

/// Slide the tab indicator onto the selected tab.
///
/// The presentation state is the single selection authority, so the target
/// tab is the one carrying the selected section. The geometry is measured
/// from the live layout of the mounted surface, and the slide itself is a
/// typed state: the progress advances one step per frame and lands exactly on
/// the measured tab.
pub(super) fn sync_tab_indicator(mut access: TabIndicatorAccess) {
    let Some(surface) = access.paint.current_surface else {
        return;
    };
    let descendants: Vec<Entity> = access.children.iter_descendants(surface).collect();
    let Some(indicator) = descendants
        .iter()
        .copied()
        .find(|entity| access.indicators.contains(*entity))
    else {
        return;
    };
    let Some(tab) = descendants.iter().copied().find(|entity| {
        access
            .tabs
            .get(*entity)
            .is_ok_and(|(tab, _, _)| tab.0 == access.state.section)
    }) else {
        return;
    };
    let Some(container) = access
        .parents
        .get(indicator)
        .ok()
        .map(|parent| parent.parent())
    else {
        return;
    };
    let (Ok(container_node), Ok(container_transform)) = (
        access.nodes.get(container),
        access.transforms.get(container),
    ) else {
        return;
    };
    let Ok((_, tab_node, tab_transform)) = access.tabs.get(tab) else {
        return;
    };
    let target_x = (tab_transform.translation.x - tab_node.size().x / 2.0)
        - (container_transform.translation.x - container_node.size().x / 2.0);
    let target_width = tab_node.size().x;
    let measured_moved = (access.motion.to_x - target_x).abs() > 0.5
        || (access.motion.to_width - target_width).abs() > 0.5;
    if access.motion.target != Some(access.state.section) {
        // A new target: slide from where the indicator currently stands. The
        // very first mount has nothing to slide from, so it lands directly.
        let first = access.motion.target.is_none();
        let from_x = if first {
            target_x
        } else {
            access.motion.current_x()
        };
        let from_width = if first {
            target_width
        } else {
            access.motion.current_width()
        };
        access.motion.target = Some(access.state.section);
        access.motion.from_x = from_x;
        access.motion.to_x = target_x;
        access.motion.from_width = from_width;
        access.motion.to_width = target_width;
        access.motion.progress = if first { 1.0 } else { 0.0 };
    } else if measured_moved {
        // The strip moved (first layout, a resize, a rebuilt surface): keep
        // the slide's origin and follow the new measurement.
        access.motion.to_x = target_x;
        access.motion.to_width = target_width;
    } else if access.motion.progress < 1.0 {
        access.motion.progress = (access.motion.progress + TAB_INDICATOR_STEP).min(1.0);
    }
    let Ok(mut node) = access.indicators.get_mut(indicator) else {
        return;
    };
    node.left = Val::Px(access.motion.current_x());
    node.width = Val::Px(access.motion.current_width());
}
