//! The inventory-page header budget derived from the root's measured frame.
//!
//! The page receives one immutable [`PageLayoutBudget`] from the composition
//! root. This fold spends only shared bands: the bottom safety band, the
//! theme's control extent and the shared rhythm tokens. It never reads window
//! pixels again and never decides a product breakpoint of its own.

use taskmanager_theme::tokens::{self, RowDensity, UiSize};
use taskmanager_ui::presentation_binding::control_measurement;
use taskmanager_ui_contract::presentation::budget::PageLayoutBudget;
use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};
use taskmanager_ui_contract::presentation::vocabulary::{BOTTOM_SAFETY_BAND, VerticalSpace};

/// The one shared rhythm charged between header groups and around the inset
/// glass surface. The fold and the scaffold spend this typed fact; neither
/// restates `tokens::SPACE_8` / `tokens::SPACE_6` as a second layout budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListPageRhythm {
    /// Shared gap between header groups, header and body, and header bands.
    pub gap: f32,
    /// Shared content inset of the one page-header glass surface.
    pub inset: f32,
}

impl ListPageRhythm {
    #[must_use]
    pub const fn shared() -> Self {
        Self {
            gap: tokens::SPACE_8.0,
            inset: tokens::SPACE_6.0,
        }
    }
}

/// One frame-local inventory header allocation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListPageBudget {
    /// Count of fixed control rows the header must reserve.
    pub rows: usize,
    /// The single rhythm spent by the fold and the scaffold.
    pub rhythm: ListPageRhythm,
    /// Whole height one optional header group charges, including its gap.
    pub optional_group_height: f32,
    /// Height available to the primary body after the header and safety band.
    /// Page-specific compositions may spend this as a floor calculation, but
    /// the scaffold must not pin the body to it: the page's own primary/elastic
    /// content owns the remaining slot.
    pub table_height: f32,
    /// Whether optional header evidence may be admitted as one whole group.
    pub admit_optional_group: bool,
}

impl ListPageBudget {
    /// Mandatory-only rung for isolated component tests. Production pages
    /// always bind [`Self::from_page_layout`].
    #[must_use]
    pub const fn mandatory_only() -> Self {
        Self {
            rows: 2,
            rhythm: ListPageRhythm::shared(),
            optional_group_height: 0.0,
            table_height: 0.0,
            admit_optional_group: false,
        }
    }

    /// Keep the mandatory header rung when a page already owns its optional
    /// evidence through a second, page-specific layout budget.
    #[must_use]
    pub const fn without_optional_group(self) -> Self {
        Self {
            rows: 2,
            table_height: self.table_height
                + self.rows.saturating_sub(2) as f32 * self.optional_group_height,
            admit_optional_group: false,
            ..self
        }
    }

    /// Derive one header budget from the measured page slot.
    ///
    /// The header is mandatory. Optional evidence is admitted only after the
    /// complete header footprint, one more control row, the gaps and the
    /// bottom safety band still leave a positive primary-table slot.
    #[must_use]
    pub fn from_page_layout(
        layout: PageLayoutBudget,
        ui_size: UiSize,
        density: RowDensity,
    ) -> Self {
        let control_extent =
            control_measurement(ControlRole::Button, ControlState::Normal, ui_size, density).extent;
        let rhythm = ListPageRhythm::shared();
        let gap = rhythm.gap;
        let header_inset = rhythm.inset * 2.0;
        let mandatory_footprint = control_extent * 2.0 + gap + header_inset;
        let safety = BOTTOM_SAFETY_BAND;
        let remaining_after_mandatory =
            (layout.content_height - mandatory_footprint - safety).max(0.0);
        let optional_footprint = control_extent + gap;
        let admit_optional_group = layout.vertical_space != VerticalSpace::Constrained
            && remaining_after_mandatory > optional_footprint;
        let rows: usize = if admit_optional_group { 3 } else { 2 };
        let header_height =
            control_extent * rows as f32 + gap * (rows.saturating_sub(1)) as f32 + header_inset;
        Self {
            rows,
            rhythm,
            optional_group_height: control_extent + gap,
            table_height: (layout.content_height - header_height - gap - safety).max(0.0),
            admit_optional_group,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/list_page_budget.rs"]
mod tests;
