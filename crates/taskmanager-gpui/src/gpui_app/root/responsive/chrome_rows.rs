//! The System dashboard's chrome rows: the fixed rows the page paints above
//! its metric slot.
//!
//! These rows are chrome, not content: the budget that reserves the metric
//! slot must know their heights, and a page-local literal makes that
//! reservation a guess — the budget then reserves space the page never
//! paints, and the metric cards stop short of the bottom of their slot.
//!
//! The heights come from the shared control ladder through the shared
//! chrome-row contract, so the row the page paints and the row the budget
//! reserves are ONE fact. The page renders each row with the reserved height
//! as its MINIMUM, so the painted row and the reserved row never disagree.

use taskmanager_theme::tokens::{self, RowDensity, UiSize};
use taskmanager_ui::presentation_binding::control_measurement;
use taskmanager_ui_contract::presentation::budget::PageLayoutBudget;
use taskmanager_ui_contract::presentation::chrome_rows::{ChromeRowRole, ChromeRowSpec};
use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};

use super::dashboard::{SECTION_HEADER_ONE_LINE_WIDTH, WINDOW_CONTROLS_ONE_LINE_WIDTH};

/// The chrome rows the System dashboard paints above its metric slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DashboardChromeRows {
    /// The section/tab row: the page's sections and their actions.
    pub section_tabs: ChromeRowSpec,
    /// The window controls row: the history range pills and their readouts.
    pub window_controls: ChromeRowSpec,
    /// The paging row: the metric paging pills.
    pub paging: ChromeRowSpec,
}

impl DashboardChromeRows {
    /// Measure the dashboard's chrome rows for one measured page slot.
    ///
    /// The line count of a row is a LABEL-WIDTH fact: the row's groups fit
    /// side by side only while the slot is wide enough for their labels, and
    /// the labels are localized copy. The page therefore owns where its rows
    /// wrap; the height each line spends comes from the shared control
    /// ladder through the shared chrome-row contract.
    pub(crate) fn from_page_layout(
        layout: PageLayoutBudget,
        ui_size: UiSize,
        density: RowDensity,
    ) -> Self {
        let width = layout.content_width.max(0.0);
        let header_lines = if width >= SECTION_HEADER_ONE_LINE_WIDTH {
            1
        } else {
            2
        };
        let window_lines = if width >= WINDOW_CONTROLS_ONE_LINE_WIDTH {
            1
        } else {
            2
        };
        Self::from_ladder(ui_size, density, header_lines, window_lines)
    }

    /// Measure the dashboard's chrome rows from the shared control ladder.
    ///
    /// Every row spends the SAME ladder role — a row of navigation pills is a
    /// row of tab controls — so no row picks a second ladder for itself. The
    /// line count is the page's own label-width fact.
    pub(crate) fn from_ladder(
        ui_size: UiSize,
        density: RowDensity,
        header_lines: u32,
        window_lines: u32,
    ) -> Self {
        let line_height =
            control_measurement(ControlRole::Tab, ControlState::Normal, ui_size, density).extent;
        let line_gap = tokens::SPACE_8.0;
        let row = |role, lines| ChromeRowSpec::measured(role, line_height, lines, line_gap);
        Self {
            section_tabs: row(ChromeRowRole::SectionTabs, header_lines),
            window_controls: row(ChromeRowRole::WindowControls, window_lines),
            paging: row(ChromeRowRole::Paging, 1),
        }
    }

    /// The height every chrome row spends together.
    pub(crate) fn total(self) -> f32 {
        self.section_tabs.height() + self.window_controls.height() + self.paging.height()
    }
}
