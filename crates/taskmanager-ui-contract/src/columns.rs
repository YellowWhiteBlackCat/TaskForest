//! Neutral process-table column vocabulary.
//!
//! Single source of truth for the column semantics every frontend table
//! agrees on: which columns exist, their stable identity tokens, canonical
//! order, default widths, numeric alignment, and hideability. Frontend column
//! enums (GPUI `SortCol`, the Iced/TUI column models) map their variants onto
//! these tokens and delegate the shared semantics here instead of carrying
//! private copies that drift.
//! The table's column-width RESIZE policy (minimum / maximum / keyboard step)
//! lives here as well: it is one policy every frontend reads, not a per-column
//! fact. A frontend's own layout floor stays with that frontend.
//!
//! The `id` tokens are spelled exactly like the persisted process-view
//! sort/hidden-columns config tokens, so persisted settings, the contract,
//! and every frontend enum agree on one string per column.

/// The process table's column-width POLICY — the single authority.
///
/// These are the RESIZE limits: how far a user may drag a column's edge, and
/// how far one keyboard step moves it. Every frontend reads these constants,
/// so the same drag behaves the same everywhere. A frontend's own LAYOUT
/// floor — how narrow a column may become when the table distributes the
/// available width — stays with that frontend: it is a layout decision, not
/// a shared policy.
pub const COLUMN_WIDTH_MIN: f32 = 10.0;
/// The widest a column may become when resizing.
pub const COLUMN_WIDTH_MAX: f32 = 1200.0;
/// The width change one keyboard resize step applies.
pub const COLUMN_WIDTH_KEYBOARD_STEP: f32 = 10.0;

/// The resize arithmetic one column-width surface applies.
///
/// The bounds live in the caller's unit: the graphical frontends use the
/// shared pixel policy, while a terminal passes its own cell policy. The
/// CLAMP and STEP semantics are shared either way, so a drag and a keyboard
/// step can never disagree about how a width moves or where it stops.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnResizePolicy {
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl ColumnResizePolicy {
    /// The shared process-column pixel policy.
    pub const PROCESS: Self = Self {
        min: COLUMN_WIDTH_MIN,
        max: COLUMN_WIDTH_MAX,
        step: COLUMN_WIDTH_KEYBOARD_STEP,
    };

    /// Clamp one width into the policy's bounds (an inverted pair is
    /// normalized so `max` never sits below `min`).
    #[must_use]
    pub fn clamp(self, width: f32) -> f32 {
        width.clamp(self.min, self.max.max(self.min))
    }

    /// Apply one pointer-drag delta from the session's anchored width.
    #[must_use]
    pub fn drag(self, current: f32, delta: f32) -> f32 {
        self.clamp(current + delta)
    }

    /// Apply `steps` keyboard resize steps.
    #[must_use]
    pub fn step(self, current: f32, steps: i32) -> f32 {
        self.clamp(current + steps as f32 * self.step)
    }
}

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
/// Threads → StartTime → Status → CPU → Memory → Swap → MemoryPss → DiskRead →
/// DiskWrite → Network → CPUTime → FDs → Nice (Swap and MemoryPss follow Memory
/// so memory resources stay adjacent). Values mirror the GPUI processes table
/// this vocabulary was extracted from; adopting frontends must not carry diverging copies.
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
        id: "MemoryPss",
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
    ProcessColumnSpec {
        id: "Network",
        default_width: 90.0,
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
