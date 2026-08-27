//! Column configuration and runtime sort/width state for the table
//! (absorption §4.5: typed `TableColumn`, keyed `ColGroup` merge).

use gpui::{Bounds, Pixels, SharedString, TextAlign, px};

/// Three-state sort indicator (absorption §4.5, replacing `ColumnSort`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SortState {
    /// No active sort on this column.
    #[default]
    Unsorted,
    /// Ascending order.
    Ascending,
    /// Descending order.
    Descending,
}

impl SortState {
    /// The next state in the cycle Unsorted → Descending → Ascending →
    /// Unsorted (absorption §4.3-E: first click sorts descending).
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            SortState::Unsorted => SortState::Descending,
            SortState::Descending => SortState::Ascending,
            SortState::Ascending => SortState::Unsorted,
        }
    }

    /// Whether this state represents an active sort.
    #[must_use]
    pub fn is_active(self) -> bool {
        !matches!(self, SortState::Unsorted)
    }
}

/// One table column configuration (typed, absorbed from gc `Column`).
#[derive(Clone, Debug)]
pub struct TableColumn {
    /// Stable identity key: widths/sort survive `refresh()` by key.
    pub key: String,
    /// The header label.
    pub name: SharedString,
    /// Initial width.
    pub width: Pixels,
    /// `Some(state)` = sortable (with that initial state); `None` = not
    /// sortable.
    pub sort: Option<SortState>,
    /// Fixed on the left; only a leading run of columns may be fixed
    /// (附录 A-1: leading-N-only support).
    pub fixed_left: bool,
    /// Whether the column width can be dragged.
    pub resizable: bool,
    /// Whether the column can be moved by dragging its header.
    pub movable: bool,
    /// Whether the column participates in column selection.
    pub selectable: bool,
    /// Cell text alignment.
    pub text_align: TextAlign,
}

impl Default for TableColumn {
    fn default() -> Self {
        Self {
            key: String::new(),
            name: SharedString::new(""),
            width: px(100.0),
            sort: None,
            fixed_left: false,
            resizable: true,
            movable: true,
            selectable: true,
            text_align: TextAlign::Left,
        }
    }
}

impl TableColumn {
    /// Create a column with a stable identity key and display name.
    pub fn new(key: impl Into<String>, name: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set the initial width.
    #[must_use]
    pub fn width(mut self, width: impl Into<Pixels>) -> Self {
        self.width = width.into();
        self
    }

    /// Enable sorting with an initial state.
    #[must_use]
    pub fn sort(mut self, sort: SortState) -> Self {
        self.sort = Some(sort);
        self
    }

    /// Enable sorting starting from `Unsorted`.
    #[must_use]
    pub fn sortable(mut self) -> Self {
        self.sort = Some(SortState::Unsorted);
        self
    }

    /// Pin the column to the left (leading-run only, see struct docs).
    #[must_use]
    pub fn fixed_left(mut self, fixed_left: bool) -> Self {
        self.fixed_left = fixed_left;
        self
    }

    /// Set whether the column width is draggable.
    #[must_use]
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Set whether the column can be moved by dragging its header.
    #[must_use]
    pub fn movable(mut self, movable: bool) -> Self {
        self.movable = movable;
        self
    }

    /// Set whether the column can be selected.
    #[must_use]
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// Right-align the cell text.
    #[must_use]
    pub fn text_right(mut self) -> Self {
        self.text_align = TextAlign::Right;
        self
    }
}

/// Runtime column state. Width and sort live here (not in the delegate's
/// [`TableColumn`]) so `refresh()` can merge them by key (附录 A-9).
#[derive(Clone, Debug)]
pub struct ColGroup {
    /// The configured column (identity + capabilities).
    pub column: TableColumn,
    /// The runtime width (dragging updates it).
    pub width: Pixels,
    /// The bounds of this column in the table after it renders.
    pub bounds: Bounds<Pixels>,
    /// The runtime sort state of this column.
    pub sort: SortState,
}

impl ColGroup {
    /// Whether this column is currently resizable.
    #[must_use]
    pub fn is_resizable(&self) -> bool {
        self.column.resizable
    }
}

pub fn leading_fixed_cols_count(groups: &[ColGroup]) -> usize {
    groups.iter().take_while(|g| g.column.fixed_left).count()
}

/// Validate the fixed-column invariant: fixed columns must form a leading
/// run (absorption 4.6-2). Non-leading fixed columns are simply not counted
/// (A-1): the return value is always the leading-run count.
#[must_use]
pub fn validate_leading_fixed(groups: &[ColGroup]) -> usize {
    leading_fixed_cols_count(groups)
}
