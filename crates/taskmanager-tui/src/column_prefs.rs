//! Applications-page column display control and the visible sort cycle
//! (ADR-027 frontend-local state).
//!
//! The column-visibility menu (`C`), the always-visible PID/Name identity
//! columns, the hidden-set-aware effective sort column, and the `s` key's
//! visible-only sort cycle all live here, together with the detail-panel
//! scroll offset and the Applications focus ring. Extracted from `lib.rs` to
//! keep the crate root under the source line budget (behavior unchanged —
//! every method stays reachable on `TuiApp`).

use taskmanager_application::AppPage;

use crate::FocusPanel;
use crate::{TuiApp, TuiSurface, TuiSurfaceKind};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, SortCol};

impl TuiApp {
    /// Scroll the inline detail panel by `delta` lines (positive = down). The
    /// renderer clamps the stored intent to the valid range (see
    /// `ui::process_details::render_process_details`), so this only stores the
    /// user's intent; it never reads the render area. Mirrors the Properties
    /// modal's scroll, which clamps
    /// to `[0, max(0, content_lines - visible_height)]`. Used by the
    /// Applications-page Ctrl+Up / Ctrl+Down chords, which never move the table
    /// cursor.
    pub fn detail_scroll_by(&mut self, delta: isize) {
        if delta >= 0 {
            self.detail_scroll = self.detail_scroll.saturating_add(delta as usize);
        } else {
            self.detail_scroll = self.detail_scroll.saturating_sub(delta.unsigned_abs());
        }
    }

    /// Reset the inline detail-panel scroll offset to 0. Called on every table
    /// selection move so a stale offset from a different process never carries
    /// into fresh content.
    pub fn detail_scroll_reset(&mut self) {
        self.detail_scroll = 0;
    }

    /// Cycle the Applications-page keyboard focus between the process table
    /// and the inline detail panel (Tab). No-op off the Applications page;
    /// page/mode changes reset the focus to the table elsewhere.
    pub fn cycle_focus_panel(&mut self) {
        if self.page() != AppPage::Applications {
            return;
        }
        self.focus_panel = self.focus_panel.next();
        let text = match self.focus_panel {
            FocusPanel::Table => "Focus: process table",
            FocusPanel::Details => "Focus: details panel (↑/↓ scroll, Esc back)",
        };
        self.report_notice(
            FeedbackSource::Navigation,
            FeedbackSeverity::Info,
            FeedbackLifecycle::SHORT,
            text,
        );
    }

    /// The toggleable process-table columns in display order (PID and Name are
    /// identity columns and stay always-visible). This is the single source the
    /// column menu renders, so the renderer and the menu can never disagree.
    #[must_use]
    pub const fn toggleable_columns() -> [SortCol; 13] {
        [
            SortCol::Cpu,
            SortCol::Memory,
            SortCol::Pss,
            SortCol::Swap,
            SortCol::User,
            SortCol::State,
            SortCol::Threads,
            SortCol::Fds,
            SortCol::Nice,
            SortCol::StartTime,
            SortCol::CpuTime,
            SortCol::DiskRead,
            SortCol::DiskWrite,
        ]
    }

    /// Toggle the column-visibility menu on the Applications page (mutually
    /// exclusive with every other overlay; the modal trap owns its keys).
    pub fn toggle_column_menu(&mut self) {
        if self.page() != AppPage::Applications {
            return;
        }
        if self.local_surface_kind() == Some(TuiSurfaceKind::ColumnMenu) {
            self.dismiss_local_surface_kind(TuiSurfaceKind::ColumnMenu);
            return;
        }
        self.open_local_surface(TuiSurface::ColumnMenu { selection: 0 });
    }

    /// Move the column-menu cursor (clamped).
    pub fn column_menu_move(&mut self, delta: isize) {
        let count = Self::toggleable_columns().len();
        if let Some(selection) = self.column_menu_selection_mut() {
            *selection = selection.saturating_add_signed(delta).min(count - 1);
        }
    }

    /// Toggle the hidden flag of the column under the menu cursor.
    pub fn column_menu_toggle(&mut self) {
        let Some(column) = self
            .column_menu_selection()
            .and_then(|selection| Self::toggleable_columns().get(selection).copied())
        else {
            return;
        };
        if !self.hidden_columns.remove(&column) {
            self.hidden_columns.insert(column);
        }
        // Hiding the active sort column moves the sort to the first visible
        // column so the arrow never points at a column that is not rendered.
        if self.hidden_columns.contains(&self.process_sort.0)
            && let Some(first_visible) = Self::toggleable_columns()
                .into_iter()
                .find(|candidate| !self.hidden_columns.contains(candidate))
        {
            self.set_process_sort_column_preserving_anchor(first_visible);
        }
        self.detail_scroll_reset();
        self.persist_process_prefs();
    }

    /// Whether the column is currently rendered (PID and Name are
    /// always-visible identity columns; every other column obeys the hide set).
    #[must_use]
    pub fn column_visible(&self, column: SortCol) -> bool {
        matches!(column, SortCol::Pid | SortCol::Name) || !self.hidden_columns.contains(&column)
    }

    /// The active sort column, redirected to the first visible column when the
    /// current one is hidden (a hidden sort column must never silently sort by
    /// an invisible key — `set_sort_column` already relocates it, this is the
    /// read-side guard).
    #[must_use]
    pub fn effective_sort_col(&self) -> SortCol {
        if self.column_visible(self.process_sort.0) {
            self.process_sort.0
        } else {
            Self::toggleable_columns()
                .into_iter()
                .find(|candidate| self.column_visible(*candidate))
                .unwrap_or(SortCol::Cpu)
        }
    }

    /// The sort-cycle order: the base display cycle (PID → Name → CPU →
    /// Memory → PSS → Swap → User → State) followed by the advanced columns
    /// (Threads / CPU time / Disk r/w / Start / Fds / Nice) — the same
    /// left-to-right order the header renders. The `s` key walks ONLY the
    /// visible columns, so hiding columns narrows the cycle (the shared shell
    /// `cycle_sort_column` would walk the full base cycle regardless of what
    /// the terminal can show). Single source for the visible sort cycle.
    const SORT_CYCLE: [SortCol; 15] = [
        SortCol::Pid,
        SortCol::Name,
        SortCol::Cpu,
        SortCol::Memory,
        SortCol::Pss,
        SortCol::Swap,
        SortCol::User,
        SortCol::State,
        SortCol::Threads,
        SortCol::CpuTime,
        SortCol::DiskRead,
        SortCol::DiskWrite,
        SortCol::StartTime,
        SortCol::Fds,
        SortCol::Nice,
    ];

    /// Advance the Applications-page sort to the next VISIBLE column, keeping
    /// the direction. Wraps from the last visible column to the first. Uses
    /// the shell's `set_sort_column` so the cursor reset + selection sync +
    /// status stay consistent with every other sort path.
    pub fn cycle_sort_column_visible(&mut self) {
        let current = self.effective_sort_col();
        let start = Self::SORT_CYCLE
            .iter()
            .position(|candidate| *candidate == current)
            .unwrap_or(0);
        let next = Self::SORT_CYCLE
            .iter()
            .cycle()
            .skip(start + 1)
            .take(Self::SORT_CYCLE.len())
            .find(|candidate| self.column_visible(**candidate))
            .copied()
            .unwrap_or(current);
        if next != current {
            self.set_process_sort_column_preserving_anchor(next);
        }
        self.persist_process_prefs();
    }

    /// TUI-owned wrapper around the shell sort direction so an Applications
    /// row keeps its identity when the ordering flips.
    pub fn toggle_sort_direction(&mut self) {
        self.toggle_process_sort_direction_preserving_anchor();
        self.persist_process_prefs();
    }
}
