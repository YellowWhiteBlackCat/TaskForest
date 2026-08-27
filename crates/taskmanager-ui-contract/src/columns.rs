//! Neutral process-table column vocabulary.
//!
//! Single source of truth for the column semantics every frontend table
//! agrees on: which columns exist, their stable identity tokens, canonical
//! order, default widths, numeric alignment, and hideability. Frontend column
//! enums (GPUI `SortCol`, the Iced/TUI column models) map their variants onto
//! these tokens and delegate the shared semantics here instead of carrying
//! private copies that drift.
//!
//! The `id` tokens are spelled exactly like the persisted process-view
//! sort/hidden-columns config tokens, so persisted settings, the contract,
//! and every frontend enum agree on one string per column.

/// One process-table column, described toolkit-neutrally.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcessColumnSpec {
    /// Stable identity token (also the persisted config spelling).
    pub id: &'static str,
    /// Default cell width in device-independent pixels.
    pub default_width: f32,
    /// Numeric columns right-align their cells so digits line up vertically.
    pub numeric: bool,
    /// Whether a column picker may hide this column. The identity column
    /// (`Name`) is always visible and therefore not hideable.
    pub hideable: bool,
}

/// The complete column inventory in canonical order: Name → User → PID →
/// Threads → StartTime → Status → CPU → Memory → Swap → DiskRead → DiskWrite →
/// CPUTime → FDs → Nice (Swap follows Memory so the two memory resources stay
/// adjacent). Values mirror the GPUI processes table this vocabulary was
/// extracted from; adopting frontends must not carry diverging copies.
pub const PROCESS_COLUMNS: &[ProcessColumnSpec] = &[
    ProcessColumnSpec {
        id: "Name",
        default_width: 120.0,
        numeric: false,
        hideable: false,
    },
    ProcessColumnSpec {
        id: "User",
        default_width: 120.0,
        numeric: false,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "PID",
        default_width: 70.0,
        numeric: true,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "Threads",
        default_width: 60.0,
        numeric: true,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "StartTime",
        default_width: 60.0,
        numeric: false,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "Status",
        default_width: 90.0,
        numeric: false,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "CPU",
        default_width: 70.0,
        numeric: true,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "Memory",
        default_width: 100.0,
        numeric: true,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "Swap",
        default_width: 100.0,
        numeric: true,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "DiskRead",
        default_width: 100.0,
        numeric: true,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "DiskWrite",
        default_width: 100.0,
        numeric: true,
        hideable: true,
    },
    // 100px fits "30d 23h" (days>0 drops minutes) with margin; 80 overflowed
    // leftward into DiskWrite for any process running >= 1 day.
    ProcessColumnSpec {
        id: "CPUTime",
        default_width: 100.0,
        numeric: true,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "FDs",
        default_width: 60.0,
        numeric: true,
        hideable: true,
    },
    ProcessColumnSpec {
        id: "Nice",
        default_width: 56.0,
        numeric: true,
        hideable: true,
    },
];

/// Look up one column by its stable token.
#[must_use]
pub fn find(id: &str) -> Option<&'static ProcessColumnSpec> {
    PROCESS_COLUMNS.iter().find(|spec| spec.id == id)
}

#[cfg(test)]
#[path = "../tests/headless/ui_columns.rs"]
mod tests;
