//! The shared chrome-row height contract: the fixed rows a page paints above
//! its content.
//!
//! A page spends a few FIXED rows before its content slot: the section tabs,
//! the window controls, the paging row. The budget that reserves the content
//! slot must know their heights. A guess reserves space the page never paints
//! — the content stops short of the bottom of its own slot — or too little,
//! and the content overflows the bottom safety band.
//!
//! This module owns the vocabulary of chrome rows and the fold that turns the
//! theme's control ladder into a row height: a chrome row is as tall as the
//! control extent its controls spend, times the number of lines it wraps
//! into, plus the gap between two lines. The ladder itself lives in
//! [`super::sizes`]; a frontend supplies its theme's axis points through its
//! binding seam and never re-numbers a row locally.

use crate::keybindings::FrontendShape;
use crate::presentation::sizes::{
    ControlMeasurement, ControlRole, ControlState, DensityPoint, UiSizePoint, resolve_size,
};

/// The semantic role of one page chrome row.
///
/// The role names the row a page paints; the ladder role its controls spend
/// is derived here, so a page never re-numbers a row and never picks a
/// second ladder for the same row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChromeRowRole {
    /// A row of navigation pills: the section/tab row of a page.
    SectionTabs,
    /// A row of range pills and the readouts beside them.
    WindowControls,
    /// A row of paging pills.
    Paging,
}

impl ChromeRowRole {
    /// The ladder role the controls of this row spend.
    #[must_use]
    pub const fn control_role(self) -> ControlRole {
        match self {
            Self::SectionTabs | Self::WindowControls | Self::Paging => ControlRole::Tab,
        }
    }
}

/// The measured facts one chrome row's height is built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChromeRowSpec {
    /// The semantic role this row plays on the page.
    pub role: ChromeRowRole,
    /// The height of ONE line of the row: the control extent its controls
    /// spend, or the taller of that extent and a readout the row carries.
    pub line_height: f32,
    /// How many lines the row wraps into.
    pub lines: u32,
    /// The gap between two wrapped lines.
    pub line_gap: f32,
}

impl ChromeRowSpec {
    /// A chrome row of ladder controls at one theme axis point.
    ///
    /// The line height is the shared control ladder's extent for the row's
    /// control role, so the row follows the theme's UI-size and density axes
    /// instead of a page-local literal.
    #[must_use]
    pub fn from_ladder(
        shape: FrontendShape,
        role: ChromeRowRole,
        state: ControlState,
        ui_size: UiSizePoint,
        density: DensityPoint,
        lines: u32,
        line_gap: f32,
    ) -> Self {
        Self {
            role,
            line_height: resolve_size(shape, role.control_role(), state, ui_size, density).extent,
            lines,
            line_gap,
        }
    }

    /// A chrome row whose line height is already measured.
    #[must_use]
    pub const fn measured(
        role: ChromeRowRole,
        line_height: f32,
        lines: u32,
        line_gap: f32,
    ) -> Self {
        Self {
            role,
            line_height,
            lines,
            line_gap,
        }
    }

    /// The measurement this row's ladder role resolved.
    #[must_use]
    pub fn measurement(
        &self,
        shape: FrontendShape,
        state: ControlState,
        ui_size: UiSizePoint,
        density: DensityPoint,
    ) -> ControlMeasurement {
        resolve_size(shape, self.role.control_role(), state, ui_size, density)
    }

    /// Raise the row's line height to a measured readout's line height.
    ///
    /// A row that carries a text readout beside its controls is as tall as
    /// the taller of the two, so the row never clips the fact it carries.
    #[must_use]
    pub fn with_readout(mut self, readout_line_height: f32) -> Self {
        if readout_line_height.is_finite() && readout_line_height > self.line_height {
            self.line_height = readout_line_height;
        }
        self
    }

    /// The height the row spends.
    ///
    /// One line is the control extent; a wrapped row adds its gap and one
    /// more line for every line it wraps into.
    #[must_use]
    pub fn height(self) -> f32 {
        let lines = self.lines.max(1);
        self.line_height * lines as f32 + self.line_gap * (lines.saturating_sub(1)) as f32
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_chrome_rows.rs"]
mod tests;
