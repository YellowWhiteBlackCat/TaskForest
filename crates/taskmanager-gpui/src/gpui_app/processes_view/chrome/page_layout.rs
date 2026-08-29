//! Typed Apps-page chrome allocation.
//!
//! The viewport boundary produces one [`PageLayoutBudget`]. This module maps
//! that global fact into page-specific presentation choices exactly once, so
//! render code never reconstructs responsive policy from unrelated booleans.

use crate::gpui_app::root::{LayoutProfile, PageLayoutBudget, VerticalSpace};
use taskmanager_theme::Length;
use taskmanager_theme::tokens;

/// Amount of process-page identity information kept in the overview band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcessOverviewPresentation {
    /// Keep both application and process counts; search shares the row.
    SummaryAndSearch,
    /// Keep the page identity and search, while the footer remains the process
    /// count authority. This returns height to the table in short windows.
    TitleAndSearch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessSearchPresentation {
    Narrow,
    Regular,
}

/// Placement of the command and filtering controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcessControlPresentation {
    /// Commands and filters are one bounded horizontal surface.
    Unified,
    /// Commands and filters use two predictable, non-overlapping bands.
    Stacked,
}

/// Inline command vocabulary. Every action excluded here remains reachable
/// from the anchored overflow menu; this enum changes placement, not ability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcessActionPresentation {
    /// Run, graceful end, columns and overflow.
    Essential,
    /// Essential commands plus force-stop as a visible primary action.
    Primary,
}

/// Whether the action strip owns a card or participates in a shared card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcessActionSurface {
    Standalone,
    Embedded,
}

/// Page-specific immutable allocation derived from the global frame budget.
///
/// It intentionally contains no persisted preference and no independent
/// booleans. Resizing creates a new value; all Apps chrome consumes the same
/// value for that frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessChromePresentation {
    overview: ProcessOverviewPresentation,
    controls: ProcessControlPresentation,
    actions: ProcessActionPresentation,
    search: ProcessSearchPresentation,
}

impl ProcessChromePresentation {
    #[must_use]
    pub const fn from_page_layout(layout: PageLayoutBudget) -> Self {
        let overview = match layout.vertical_space {
            VerticalSpace::Constrained => ProcessOverviewPresentation::TitleAndSearch,
            VerticalSpace::Standard | VerticalSpace::Generous => {
                ProcessOverviewPresentation::SummaryAndSearch
            }
        };
        let controls = match layout.profile {
            LayoutProfile::Wide => ProcessControlPresentation::Unified,
            LayoutProfile::UltraCompact | LayoutProfile::Compact | LayoutProfile::Standard => {
                ProcessControlPresentation::Stacked
            }
        };
        let actions = match (layout.profile, layout.vertical_space) {
            (
                LayoutProfile::Standard | LayoutProfile::Wide,
                VerticalSpace::Standard | VerticalSpace::Generous,
            ) => ProcessActionPresentation::Primary,
            (
                LayoutProfile::UltraCompact
                | LayoutProfile::Compact
                | LayoutProfile::Standard
                | LayoutProfile::Wide,
                VerticalSpace::Constrained,
            )
            | (
                LayoutProfile::UltraCompact | LayoutProfile::Compact,
                VerticalSpace::Standard | VerticalSpace::Generous,
            ) => ProcessActionPresentation::Essential,
        };
        let search = match layout.profile {
            LayoutProfile::UltraCompact | LayoutProfile::Compact => {
                ProcessSearchPresentation::Narrow
            }
            LayoutProfile::Standard | LayoutProfile::Wide => ProcessSearchPresentation::Regular,
        };
        Self {
            overview,
            controls,
            actions,
            search,
        }
    }

    pub(super) const fn overview(self) -> ProcessOverviewPresentation {
        self.overview
    }

    pub(super) const fn controls(self) -> ProcessControlPresentation {
        self.controls
    }

    pub(super) const fn actions(self) -> ProcessActionPresentation {
        self.actions
    }

    pub(super) const fn action_surface(self) -> ProcessActionSurface {
        match self.controls {
            ProcessControlPresentation::Unified => ProcessActionSurface::Embedded,
            ProcessControlPresentation::Stacked => ProcessActionSurface::Standalone,
        }
    }

    pub(super) const fn band_gap(self) -> Length {
        match self.controls {
            ProcessControlPresentation::Unified => tokens::SPACE_8,
            ProcessControlPresentation::Stacked => tokens::SPACE_6,
        }
    }

    pub(super) const fn control_gap(self) -> Length {
        match self.controls {
            ProcessControlPresentation::Unified => tokens::SPACE_12,
            ProcessControlPresentation::Stacked => tokens::SPACE_8,
        }
    }

    pub(super) const fn search_width(self) -> f32 {
        match self.search {
            ProcessSearchPresentation::Narrow => 260.0,
            ProcessSearchPresentation::Regular => 320.0,
        }
    }
}
