//! Applications column-width overrides, the header-drag interaction, the
//! keyboard/menu stepper path, and the persisted config token.
//!
//! The process table's semantic widths (defaults, alignment, hideability) are
//! contract truth (`taskmanager_ui_contract::PROCESS_COLUMNS`, consumed by
//! `ui::applications`); this module owns the session-local overrides a
//! header-edge drag or a stepper activation produces. Overrides carry across
//! sessions through the shared `Config::process_col_widths` token (the same
//! vocabulary GPUI persists): every change commits through the standard
//! configuration channel, and the startup snapshot restores the token into
//! the live override set.
//!
//! Interaction model: the header cell's trailing edge is a `mouse_area` strip
//! whose press opens a drag session. iced's `mouse_area` only reports motion
//! while the pointer is inside its own bounds, so the live drag is fed by the
//! raw pointer subscription mounted while a session exists
//! (`app::subscription`): cursor moves derive `start_width + dx`, the left
//! button's release closes the session and keeps the width. The
//! keyboard-accessible path is the column menu's per-column stepper buttons
//! (`ui::column_menu`): focusable controls that publish the same
//! [`Message::ResizeProcessColumn`] transition in fixed steps.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use iced::Point;
use taskmanager_core::core::config::ColumnWidthConfig;
use taskmanager_shell::SortCol;

use super::super::{IcedApp, Message};
use super::dispatch::UpdateDispatch;

/// Smallest storable column width. Below the contract's narrowest default
/// (56px) a header caption stops fitting; 40 keeps a dragged column readable
/// instead of collapsible to a sliver.
pub(crate) const MIN_PROCESS_COLUMN_WIDTH: f32 = 40.0;

/// Largest storable column width. Five times the widest contract default
/// (120px) leaves room for long process names without letting one column
/// push every other off a wide viewport.
pub(crate) const MAX_PROCESS_COLUMN_WIDTH: f32 = 600.0;

/// Session-local width overrides for the Applications table columns: the
/// results of header-edge drags, keyed by the shell sort identity. A column
/// without an entry renders its contract default.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ColumnWidthOverrides {
    widths: HashMap<SortCol, f32>,
}

impl ColumnWidthOverrides {
    /// The stored override for this column, if any. Callers fall back to the
    /// contract default themselves so this type never duplicates contract
    /// truth.
    #[must_use]
    pub(crate) fn get(&self, column: SortCol) -> Option<f32> {
        self.widths.get(&column).copied()
    }

    /// Store one column's width. The value is rounded to whole pixels (header
    /// and body sum these widths; whole pixels keep the sticky header
    /// pixel-aligned without subpixel churn) and clamped to the sizing
    /// domain. A non-finite input is ignored outright — it can never
    /// legitimately reach layout code.
    pub(crate) fn set(&mut self, column: SortCol, width: f32) {
        if !width.is_finite() {
            return;
        }
        self.widths.insert(
            column,
            width
                .round()
                .clamp(MIN_PROCESS_COLUMN_WIDTH, MAX_PROCESS_COLUMN_WIDTH),
        );
    }

    /// Iterate the stored `(column, width)` overrides in unspecified order;
    /// callers that need stable iteration (invalidation keys) sort a
    /// collected copy.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (SortCol, f32)> + '_ {
        self.widths.iter().map(|(column, width)| (*column, *width))
    }

    /// Drop every stored override (the column menu's reset action): columns
    /// fall back to their contract defaults and the persisted token empties.
    pub(crate) fn clear(&mut self) {
        self.widths.clear();
    }

    /// Serialize the overrides into the persisted
    /// `Config::process_col_widths` token form: the contract-id spelling,
    /// whole-pixel values. Only resizable columns are emitted — the reducer
    /// gates the identity column out, but the filter stays defensive (a
    /// future caller cannot smuggle a `Name` width onto disk). Output is
    /// sorted by token so an unchanged layout serializes byte-identically
    /// across runs (HashMap iteration order is random), mirroring the GPUI
    /// writer.
    pub(crate) fn to_config(&self) -> Vec<ColumnWidthConfig> {
        let mut entries: Vec<_> = self
            .widths
            .iter()
            .filter(|(column, _)| crate::ui::applications::column_resizable(**column))
            .map(|(column, width)| ColumnWidthConfig {
                column: crate::ui::applications::sort_col_contract_id(*column).to_string(),
                width: *width,
            })
            .collect();
        entries.sort_by(|a, b| a.column.cmp(&b.column));
        entries
    }

    /// Parse the persisted token form back into overrides. Graceful on every
    /// failure class: unknown tokens, non-resizable columns (e.g. a stale
    /// `Name` entry), and non-finite widths are dropped individually — never
    /// a panic, never fabricated state. A finite out-of-domain width clamps
    /// into the sizing domain through [`Self::set`] (a hand-edited config
    /// cannot blow out the table), and the first occurrence wins on a
    /// duplicate token (only reachable in a hand-edited file; the save side
    /// is deduplicated + sorted).
    pub(crate) fn from_config(entries: &[ColumnWidthConfig]) -> Self {
        let mut widths = Self::default();
        for entry in entries {
            if !entry.width.is_finite() {
                continue;
            }
            let Some(column) = crate::ui::applications::sort_col_from_contract_id(&entry.column)
                .filter(|column| crate::ui::applications::column_resizable(*column))
            else {
                continue;
            };
            if widths.get(column).is_some() {
                continue;
            }
            widths.set(column, entry.width);
        }
        widths
    }
}

/// Width delta applied by one keyboard/menu stepper activation. 16px is coarse
/// enough to cross the sizing domain in a few presses yet fine enough to land
/// a header caption precisely; the store-side clamp bounds every step.
pub(crate) const PROCESS_COLUMN_KEYBOARD_STEP: f32 = 16.0;

/// The absolute width one stepper activation requests for a column currently
/// rendering at `current`. The view publishes the result through the existing
/// [`Message::ResizeProcessColumn`] transition (no new state machine), so the
/// reducer's clamp domain saturates steps that run past an edge.
#[must_use]
pub(crate) fn keyboard_resize_width(current: f32, wider: bool) -> f32 {
    current
        + if wider {
            PROCESS_COLUMN_KEYBOARD_STEP
        } else {
            -PROCESS_COLUMN_KEYBOARD_STEP
        }
}

/// One open header-edge drag: the column being sized, the pointer-space
/// anchor, and the column's width when the session opened. `origin_x` is
/// anchored by the FIRST tracked pointer motion (a `mouse_area` press message
/// carries no coordinates), so the drag is delta-based from that sample.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcessColumnDrag {
    pub(crate) column: SortCol,
    origin_x: Option<f32>,
    start_width: f32,
}

impl ProcessColumnDrag {
    /// The window-space x the drag is measured from, once the first tracked
    /// pointer motion anchored it. Test seam for the anchor behavior.
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub(crate) fn origin_x(&self) -> Option<f32> {
        self.origin_x
    }
}

/// The whole column-sizing state of the Applications table: the durable
/// session-local overrides, the transient drag session while a header edge
/// is held, and the stepper path's persistence debounce gate (see
/// [`StepperPersistGate`]).
#[derive(Clone, Debug, Default)]
pub(crate) struct ProcessColumnSizing {
    pub(crate) overrides: ColumnWidthOverrides,
    pub(crate) drag: Option<ProcessColumnDrag>,
    pub(crate) stepper_gate: StepperPersistGate,
}

/// Debounce bookkeeping for the keyboard/menu stepper's config commits.
/// Keyboard auto-repeat re-fires [`Message::ResizeProcessColumn`] much
/// faster than any user needs a disk write per step, so an activation
/// landing inside [`PROCESS_COLUMN_STEPPER_COALESCE`] of the last
/// straight-through commit only marks `pending`; the poll tick (and the
/// quit path) are the flush points that land the deferred commit once the
/// window has elapsed. The live width override always follows every
/// activation — only the persistence is coalesced.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StepperPersistGate {
    pub(crate) last_commit: Option<Instant>,
    pub(crate) pending: bool,
}

/// Coalescing window for stepper-driven width persistence (see
/// [`StepperPersistGate`]): a held key commits at most once per window, and
/// the final width lands within one poll tick of the last repeat.
pub(crate) const PROCESS_COLUMN_STEPPER_COALESCE: Duration = Duration::from_millis(250);

/// Whether a stepper activation may commit straight through at `now` (no
/// commit has happened yet, or the previous one is at least one coalescing
/// window old). Pure so the debounce rule is unit-tested headlessly.
#[must_use]
pub(crate) fn stepper_commit_now(last_commit: Option<Instant>, now: Instant) -> bool {
    last_commit.is_none_or(|at| now.duration_since(at) >= PROCESS_COLUMN_STEPPER_COALESCE)
}

/// Whether a deferred (`pending`) commit's flush point has arrived: the
/// window must have elapsed since the last straight-through commit. Pure so
/// the flush rule is unit-tested headlessly.
#[must_use]
pub(crate) fn stepper_flush_due(gate: StepperPersistGate, now: Instant) -> bool {
    gate.pending && stepper_commit_now(gate.last_commit, now)
}

impl ProcessColumnSizing {
    /// A drag release, menu reset, or quit flushed the token directly: any
    /// deferred stepper commit is subsumed, and the next isolated activation
    /// commits straight through again.
    pub(crate) fn note_direct_persist(&mut self) {
        self.stepper_gate = StepperPersistGate::default();
    }
}

impl IcedApp {
    pub(super) fn reduce_process_column_message(&mut self, message: Message) -> UpdateDispatch {
        match message {
            Message::BeginProcessColumnDrag {
                column,
                start_width,
            } => self.begin_process_column_drag(column, start_width),
            Message::ProcessColumnDragMoved(position) => {
                self.move_process_column_drag(position);
            }
            Message::ProcessColumnDragReleased => {
                self.process_column_sizing.drag = None;
                // The drag stored its final width on the last tracked motion;
                // release is the single persistence point for the session (a
                // commit per motion would flood the configuration worker).
                self.persist_process_column_widths();
                self.process_column_sizing.note_direct_persist();
            }
            Message::ResizeProcessColumn { column, width } => {
                self.resize_process_column(column, width);
            }
            // route() assigns exactly the four variants above to this domain.
            _ => return UpdateDispatch::none(),
        }
        UpdateDispatch::none()
    }

    /// Open a drag session from a header-edge press. The identity column is
    /// never resizable (contract parity), and a hostile start width cannot
    /// seed a session.
    fn begin_process_column_drag(&mut self, column: SortCol, start_width: f32) {
        if !crate::ui::applications::column_resizable(column) || !start_width.is_finite() {
            return;
        }
        self.process_column_sizing.drag = Some(ProcessColumnDrag {
            column,
            origin_x: None,
            start_width: start_width.max(0.0),
        });
    }

    /// Advance the open drag session to a tracked pointer position. The first
    /// motion only anchors the delta origin; later motions store the clamped
    /// width override directly (live resize — the table rebuild is bounded by
    /// the virtual row window).
    fn move_process_column_drag(&mut self, position: Point) {
        let Some(drag) = self.process_column_sizing.drag else {
            return; // stale motion after release: no session, no effect
        };
        if !position.x.is_finite() {
            return;
        }
        let Some(origin_x) = drag.origin_x else {
            self.process_column_sizing.drag = Some(ProcessColumnDrag {
                origin_x: Some(position.x),
                ..drag
            });
            return;
        };
        let width = drag.start_width + (position.x - origin_x);
        self.process_column_sizing.overrides.set(drag.column, width);
    }

    /// Store one width override directly (the drag path's transition, and the
    /// transition the keyboard/menu stepper path publishes), then commit the
    /// override set through the shared configuration channel so the width
    /// survives a restart. The stepper path arrives here once per activation,
    /// and keyboard auto-repeat can re-fire it dozens of times per second, so
    /// the commit goes through the coalescing gate
    /// ([`PROCESS_COLUMN_STEPPER_COALESCE`]): an isolated activation commits
    /// immediately, repeats inside the window defer to the poll-tick flush,
    /// and the live override follows every activation either way.
    fn resize_process_column(&mut self, column: SortCol, width: f32) {
        if crate::ui::applications::column_resizable(column) {
            self.process_column_sizing.overrides.set(column, width);
            let now = Instant::now();
            if stepper_commit_now(self.process_column_sizing.stepper_gate.last_commit, now) {
                self.process_column_sizing.stepper_gate = StepperPersistGate {
                    last_commit: Some(now),
                    pending: false,
                };
                self.persist_process_column_widths();
            } else {
                self.process_column_sizing.stepper_gate.pending = true;
            }
        }
    }

    /// Poll-tick flush point for a deferred stepper commit: lands it only
    /// after the coalescing window has elapsed, so a held key commits at
    /// most once per window while the width itself keeps following every
    /// repeat. A no-op when nothing is deferred.
    pub(crate) fn poll_process_column_persist(&mut self) {
        let now = Instant::now();
        if !stepper_flush_due(self.process_column_sizing.stepper_gate, now) {
            return;
        }
        self.process_column_sizing.stepper_gate = StepperPersistGate {
            last_commit: Some(now),
            pending: false,
        };
        self.persist_process_column_widths();
    }

    /// Publish the live column-width overrides into the shared configuration
    /// draft and commit it through the standard settings channel (the same
    /// path every persisted preference takes). The worker's field-level merge
    /// keeps the width token disjoint from concurrent preference edits; an
    /// unchanged layout short-circuits before submitting.
    pub(crate) fn persist_process_column_widths(&mut self) {
        let widths = self.process_column_sizing.overrides.to_config();
        let mut config = self.config_draft();
        if config.process_col_widths == widths {
            return;
        }
        config.process_col_widths = widths;
        self.commit_config_draft(config);
    }

    /// Resolved render width of one Applications column this frame: the
    /// session-local drag override when present, the contract default
    /// otherwise. Header and body cells must both resolve through here (or
    /// the equivalent `RowRender` seam) or the sticky header drifts. The
    /// column menu's stepper row derives its per-step target width from this
    /// same read, so the keyboard path and the geometry share one rule.
    #[must_use]
    pub(crate) fn process_column_width(&self, column: SortCol) -> f32 {
        self.process_column_sizing
            .overrides
            .get(column)
            .unwrap_or_else(|| crate::ui::applications::column_width(column))
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/app/column_resize_tests.rs"]
mod tests;
