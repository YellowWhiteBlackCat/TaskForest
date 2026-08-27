//! Typed column vocabulary for the Iced inventory tables.
//!
//! Services / Users / Startup describe their columns with these specs instead
//! of scattering bare `Length::Fixed(..)` literals through header and body
//! builders. The vocabulary mirrors the toolkit-neutral process-column
//! contract (`taskmanager_ui_contract::columns`: id / width / alignment /
//! hideability); the process table itself consumes that contract directly
//! (see `ui/applications.rs`), while these page-local specs stay page-owned
//! because their semantics have no cross-frontend contract.

use iced::Length;
use iced::alignment::Horizontal;

/// Width contract of one column: a fixed pixel extent or the single flexible
/// remainder column that absorbs the viewport's leftover width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ColumnWidth {
    Fixed(f32),
    Fill,
}

impl ColumnWidth {
    /// The `iced` length this contract renders as.
    #[must_use]
    pub(crate) const fn length(self) -> Length {
        match self {
            Self::Fixed(width) => Length::Fixed(width),
            Self::Fill => Length::Fill,
        }
    }

    /// The fixed pixel extent; a Fill column has no intrinsic extent and
    /// reports 0.0 (composited-derivation callers combine it with their own
    /// fixed extents and gutters).
    #[must_use]
    pub(crate) const fn fixed_px(self) -> f32 {
        match self {
            Self::Fixed(width) => width,
            Self::Fill => 0.0,
        }
    }
}

/// One typed column description shared by a table's header row and every body
/// row: header and body cells read the SAME spec, which is what keeps their
/// boundaries pixel-aligned now that the header sits outside the body
/// scrollable (sticky header).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TableColumn {
    /// Stable identity token for the column within its page.
    pub(crate) id: &'static str,
    /// Shared-catalog i18n key of the header caption (rendered via `t`).
    pub(crate) label: &'static str,
    /// Width contract shared by the header cell and the body cells.
    pub(crate) width: ColumnWidth,
    /// Horizontal cell alignment. Numeric columns right-align so digits line
    /// up vertically (contract parity); text columns stay left-aligned.
    pub(crate) alignment: Horizontal,
    /// Reserved stable field for the inventory-table column-drag interaction:
    /// whether a resize handle may mount on this column. The Applications
    /// process table sizes through its own contract kit instead
    /// (`app::update::columns` overrides + the header edge in
    /// `ui::applications`); this flag remains the mounting gate for the
    /// Services / Users / Startup tables, whose planned
    /// `Message::ResizeTableColumn { table, id, width }` reducer (plus a
    /// persisted width override per `id`) will read it. Fill columns are
    /// never resizable (they have no intrinsic width to drag), mirroring the
    /// contract's identity-column rule.
    pub(crate) resizable: bool,
}

impl TableColumn {
    /// A left-aligned text column (labels, states, free-form values). Every
    /// current inventory column is textual; a numeric info column will add a
    /// right-aligning sibling constructor mirroring the contract's `numeric`
    /// flag instead of growing per-page literals back.
    pub(crate) const fn text(id: &'static str, label: &'static str, width: ColumnWidth) -> Self {
        Self {
            id,
            label,
            width,
            alignment: Horizontal::Left,
            resizable: matches!(width, ColumnWidth::Fixed(_)),
        }
    }

    /// The `iced` length of this column's header and body cells.
    #[must_use]
    pub(crate) const fn length(self) -> Length {
        self.width.length()
    }
}
