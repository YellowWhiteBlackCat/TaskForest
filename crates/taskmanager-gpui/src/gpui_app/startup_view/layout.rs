//! Startup-page allocation projected from the shared viewport budget.

use gpui::{AnyElement, Div, InteractiveElement, IntoElement, ParentElement, Styled, div, px};

use crate::core::startup::StartupBootEvidenceSnapshot;
use crate::gpui_app::list_view;
use crate::gpui_app::root::responsive::{LayoutProfile, PageLayoutBudget, VerticalSpace};
use crate::gpui_app::theme::{Theme, tokens};

use super::boot_evidence::boot_timeline_block;

const STARTUP_TABLE_MIN_HEIGHT: f32 = 168.0;
const STARTUP_TIMELINE_SIDE_WIDTH: f32 = 480.0;

/// Exhaustive placement of the boot timeline within the Startup page.
///
/// The value is derived from the frame's shared layout budget and never stored
/// as a resize-sensitive boolean on `RootView`. The row limit is part of the
/// allocation so a valid, unusually long critical chain cannot silently turn
/// secondary evidence back into unbounded page chrome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelinePresentation {
    Collapsed,
    Expanded { row_limit: usize },
    SidePanel { row_limit: usize },
}

/// Startup-specific projection of the shared viewport budget. It makes the
/// table minimum and warning density explicit while preserving the independent
/// horizontal and vertical axes from [`PageLayoutBudget`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartupPageBudget {
    pub(crate) timeline: TimelinePresentation,
    pub(crate) source_notice: list_view::SourceNoticePresentation,
    pub(crate) table_min_height: f32,
}

impl StartupPageBudget {
    #[must_use]
    pub const fn from_page_layout(layout: PageLayoutBudget) -> Self {
        let timeline = match (layout.profile, layout.vertical_space) {
            (_, VerticalSpace::Constrained) => TimelinePresentation::Collapsed,
            (LayoutProfile::UltraCompact, VerticalSpace::Standard) => {
                TimelinePresentation::Expanded { row_limit: 2 }
            }
            (LayoutProfile::Compact, VerticalSpace::Standard) => {
                TimelinePresentation::Expanded { row_limit: 3 }
            }
            (LayoutProfile::Standard, VerticalSpace::Standard) => {
                TimelinePresentation::Expanded { row_limit: 4 }
            }
            (LayoutProfile::UltraCompact, VerticalSpace::Generous) => {
                TimelinePresentation::Expanded { row_limit: 4 }
            }
            (LayoutProfile::Compact, VerticalSpace::Generous) => {
                TimelinePresentation::Expanded { row_limit: 6 }
            }
            (LayoutProfile::Standard, VerticalSpace::Generous) => {
                TimelinePresentation::Expanded { row_limit: 8 }
            }
            (LayoutProfile::Wide, VerticalSpace::Standard) => {
                TimelinePresentation::SidePanel { row_limit: 10 }
            }
            (LayoutProfile::Wide, VerticalSpace::Generous) => {
                TimelinePresentation::SidePanel { row_limit: 16 }
            }
        };
        let source_notice = match layout.vertical_space {
            VerticalSpace::Constrained => list_view::SourceNoticePresentation::Compact,
            VerticalSpace::Standard | VerticalSpace::Generous => {
                list_view::SourceNoticePresentation::Standard
            }
        };
        Self {
            timeline,
            source_notice,
            table_min_height: STARTUP_TABLE_MIN_HEIGHT,
        }
    }
}

/// Compose the already-built primary table region with the secondary boot
/// evidence according to one exhaustive presentation. Missing evidence simply
/// leaves the primary region at full size; it never reserves an empty rail.
pub(super) fn compose_content(
    theme: &Theme,
    evidence: Option<&StartupBootEvidenceSnapshot>,
    baseline: Option<&crate::core::BootTimeline>,
    layout: StartupPageBudget,
    primary: Div,
) -> AnyElement {
    match layout.timeline {
        TimelinePresentation::Collapsed => primary.into_any_element(),
        TimelinePresentation::Expanded { row_limit } => {
            let timeline = boot_timeline_block(theme, evidence, baseline, row_limit);
            div()
                .debug_selector(|| "tm-startup-stacked-content".to_string())
                .size_full()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .gap(tokens::SPACE_8)
                .child(primary)
                .children(timeline.map(|timeline| {
                    div()
                        .debug_selector(|| "tm-startup-timeline-expanded".to_string())
                        .flex_shrink_0()
                        .child(timeline)
                }))
                .into_any_element()
        }
        TimelinePresentation::SidePanel { row_limit } => {
            let timeline = boot_timeline_block(theme, evidence, baseline, row_limit);
            div()
                .debug_selector(|| "tm-startup-split-content".to_string())
                .size_full()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .flex()
                .flex_row()
                .gap(tokens::SPACE_12)
                .child(primary)
                .children(timeline.map(|timeline| {
                    div()
                        .debug_selector(|| "tm-startup-timeline-side-panel".to_string())
                        .w(px(STARTUP_TIMELINE_SIDE_WIDTH))
                        .min_w(px(STARTUP_TIMELINE_SIDE_WIDTH))
                        .max_w(px(STARTUP_TIMELINE_SIDE_WIDTH))
                        .h_full()
                        .min_h(px(0.0))
                        .overflow_hidden()
                        .child(timeline)
                }))
                .into_any_element()
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_startup_view_layout_budget_tests.rs"]
mod tests;
