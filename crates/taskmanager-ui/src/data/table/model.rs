//! Typed table selection, events and visible-range state.

use std::ops::Range;

use gpui::Pixels;

use super::SortState;

/// Row XOR column XOR none; contradictory selection states are unrepresentable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableSelection {
    Row(usize),
    Column(usize),
    None,
}

impl TableSelection {
    #[must_use]
    pub fn row(&self) -> Option<usize> {
        match self {
            Self::Row(index) => Some(*index),
            Self::Column(_) | Self::None => None,
        }
    }

    #[must_use]
    pub fn column(&self) -> Option<usize> {
        match self {
            Self::Column(index) => Some(*index),
            Self::Row(_) | Self::None => None,
        }
    }
}

/// Typed table events.
#[derive(Clone, Debug)]
pub enum TableEvent {
    SelectRow(usize),
    DoubleClickedRow(usize),
    ActivateRow(usize),
    SelectColumn(usize),
    SortChanged { col_ix: usize, sort: SortState },
    ColumnWidthsChanged(Vec<Pixels>),
    MoveColumn(usize, usize),
}

/// The visible row/column ranges of the table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableVisibleRange {
    pub(super) rows: Range<usize>,
    pub(super) cols: Range<usize>,
}

impl TableVisibleRange {
    #[must_use]
    pub fn rows(&self) -> &Range<usize> {
        &self.rows
    }

    #[must_use]
    pub fn cols(&self) -> &Range<usize> {
        &self.cols
    }
}
