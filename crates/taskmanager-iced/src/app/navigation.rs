//! The Applications-page visible-row keyboard navigation (grouped/tree view
//! modes): the visual-cursor moves, the cursor↔shared-selection sync, and the
//! group/tree expand-collapse toggles, extracted from [`super`] so the state
//! module stays under the repository's source-size budget. The keyboard paths
//! consume the same visible-row projection the renderer draws (ADR-020).

use taskmanager_application::{AppPage, KeyCode, Modifiers, PlatformEffect};
use taskmanager_shell::ShellKeyEvent;

use super::viewport_state::ViewportRegion;
use super::*;

impl IcedApp {
    /// Ctrl+C: copy the current selected row's summary through the shared
    /// shell seam (the same payload the TUI's `y` writes via OSC 52). The
    /// shared router/help contract carries the chord; the clipboard write is
    /// renderer-owned. `None` when no row is selected, an input owns the
    /// keyboard, or a modal is open.
    pub(super) fn copy_selected_row_summary(
        &mut self,
        event: &ShellKeyEvent,
    ) -> Option<iced::Task<Message>> {
        if event.key != KeyCode::C
            || !event.modifiers.control
            || self.shell.search_active()
            || self.modal_open()
        {
            return None;
        }
        let summary = self.shell.selected_row_summary()?;
        self.shell.report_notice(
            FeedbackSource::Clipboard,
            FeedbackSeverity::Success,
            FeedbackLifecycle::SHORT,
            format!(
                "Selected Row {}",
                taskmanager_application::i18n::t("common.copied")
            ),
        );
        Some(iced::clipboard::write(summary))
    }

    pub(super) fn handle_fixed_message(&mut self, event: ShellKeyEvent) -> Option<PlatformEffect> {
        if event.key == KeyCode::Escape {
            let scope = self.input_scope();
            match scope {
                InputScope::SharedSurface(_) => self.shell.dismiss_overlay(),
                InputScope::LocalSurface(_) => self.close_local_modals(),
                InputScope::ServiceLog => self.shell.close_service_log(),
                InputScope::Help | InputScope::Suggestions => self.shell.dismiss_overlay(),
                InputScope::ContextMenu(_) => self.close_context_menus(),
                InputScope::Search => self.shell.close_search(),
                InputScope::Content => {}
            }
            if !matches!(scope, InputScope::Content) {
                return None;
            }
        }

        if self.alerts_page_open() && event.key == KeyCode::Escape {
            // Bare Escape is otherwise unbound with no overlay open, so the
            // frontend-local alerts route owns it to return to the shared page.
            self.close_alerts_page();
            None
        } else if event.key == KeyCode::Digit8
            && event.modifiers == Modifiers::ALT
            && !self.modal_open()
            && !self.shell.search_active()
        {
            // The shared router registers ShowAlerts on Alt+8; the alerts
            // route is frontend-local, so this frontend consumes the chord
            // before the shell dispatch and opens its own page (the reducer's
            // OpenAlerts acknowledgment stays for the other shapes).
            self.open_alerts_page();
            None
        } else if event.key == KeyCode::F1 && event.modifiers == Modifiers::NONE {
            self.close_context_menus();
            self.close_local_modals();
            self.shell.toggle_help();
            None
        } else if event.key == KeyCode::F9 && event.modifiers == Modifiers::NONE {
            self.performance.sidebar_visible = !self.performance.sidebar_visible;
            None
        } else {
            self.handle_fixed_key(event)
        }
    }

    pub(super) fn handle_fixed_key(&mut self, event: ShellKeyEvent) -> Option<PlatformEffect> {
        if self.visual_nav_intercepts(event) {
            return None;
        }
        // The shell's local fixed-key bindings (modal overlays, arrows, the
        // shared command router); the effect comes back for queueing.
        self.shell.handle_local_key(event).into_effect()
    }

    /// Whether the fixed key was consumed by the Applications-page visual
    /// hierarchy. Up/Down/PageUp/PageDown move the
    /// cursor over the visible-row projection — the same rows the renderer
    /// draws (ADR-020 render-time projection) — and Left/Right expand or
    /// collapse the aggregate/process row at the cursor (the shared bare-arrow
    /// tree matrix; the GPUI row handler executes the same rules, pinned by
    /// behavior tests on both frontends). Every
    /// other page keeps the shared shell `move_selection` path.
    fn visual_nav_intercepts(&mut self, event: ShellKeyEvent) -> bool {
        if self.shell.page() != AppPage::Applications {
            return false;
        }
        // Mirrors the shell's own modal precedence: while a confirmation, the
        // help/suggestions sheets, or the search field is open the arrow keys
        // belong to that surface, never to the table.
        if self.shell.pending_end().is_some()
            || self.shell.help_open()
            || self.shell.suggestions_open()
            || self.shell.search_active()
        {
            return false;
        }
        match event.key {
            KeyCode::ArrowUp
            | KeyCode::ArrowDown
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => {
                self.move_visual_selection(event.key);
                true
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                self.toggle_at_visual_cursor(event.key == KeyCode::ArrowRight);
                true
            }
            _ => false,
        }
    }

    /// Move the shared selection over the visible-row projection (the render
    /// order of the grouped/tree table), mapping each visual step back to the
    /// flat process-list index the shell cursor expects. Page keys step by the
    /// shared page size. The stored [`Self::visual_cursor`] is authoritative
    /// (a header and its main member share one flat index, so the cursor
    /// cannot be re-derived from `shell.selected`).
    fn move_visual_selection(&mut self, key: KeyCode) {
        let target = {
            let projection = self.projected_rows();
            if projection.is_empty() {
                None
            } else {
                let current = self
                    .process_presentation
                    .visual_cursor
                    .min(projection.len().saturating_sub(1));
                let step: usize = if matches!(key, KeyCode::PageUp | KeyCode::PageDown) {
                    10
                } else {
                    1
                };
                let target = match key {
                    KeyCode::Home => 0,
                    KeyCode::End => projection.len().saturating_sub(1),
                    KeyCode::ArrowUp | KeyCode::PageUp => current.saturating_sub(step),
                    _ => (current + step).min(projection.len().saturating_sub(1)),
                };
                projection
                    .row_at(target)
                    .map(|row| (target, row.flat_index(), row.row_key()))
            }
        };
        let Some((target, flat_index, row_key)) = target else {
            self.shell.selected = 0;
            self.process_presentation.visual_cursor = 0;
            return;
        };
        match row_key {
            Some(taskmanager_shell::ProcessRowKey::Application(root_pid)) => {
                let _ = self.shell.select_application_row(root_pid, flat_index);
            }
            Some(taskmanager_shell::ProcessRowKey::Process(_)) => {
                let _ = self.shell.select_row(flat_index);
            }
            Some(taskmanager_shell::ProcessRowKey::Category(_)) | None => {
                self.shell.clear_process_selection();
            }
        }
        self.process_presentation.visual_cursor = target;
    }

    /// Re-derive the visual cursor from the shared selection after a
    /// selection-affecting event that did not move it explicitly (page/mode/
    /// sort/query changes and row clicks). The cursor lands on the first
    /// rendered row with the selected flat index — for a group whose main
    /// process is selected this is its header, which is exactly the row the
    /// click painted.
    pub(super) fn sync_visual_cursor(&mut self) {
        let visual_cursor = {
            let projection = self.projected_rows();
            self.shell
                .selected_process_row
                .and_then(|selected| {
                    projection
                        .rows()
                        .iter()
                        .position(|row| row.row_key() == Some(selected))
                })
                .or_else(|| projection.visual_index_of_flat(self.shell.selected))
                .unwrap_or(0)
        };
        self.process_presentation.visual_cursor = visual_cursor;
    }

    /// Expand (Right) or collapse (Left) the row at the visual cursor. A
    /// group header toggles its group; a tree node toggles its subtree; Left
    /// on an already-collapsed tree node moves the cursor up to its parent
    /// (the same structural-arrow fold the GPUI row handler executes; parity
    /// is pinned by behavior tests on both frontends).
    fn toggle_at_visual_cursor(&mut self, expand: bool) {
        let Some(row) = ({
            let projection = self.projected_rows();
            let current = self
                .process_presentation
                .visual_cursor
                .min(projection.len().saturating_sub(1));
            projection.row_at(current).cloned()
        }) else {
            return;
        };
        use crate::ui::process_projection::ProjectedRow;
        let flat_index = row.flat_index();
        match row {
            ProjectedRow::GroupHeader {
                expansion_key,
                expanded,
                row_key,
                flat_index,
                ..
            } => {
                if expand != expanded {
                    if !self
                        .process_presentation
                        .expanded_groups
                        .remove(&expansion_key)
                    {
                        self.process_presentation
                            .expanded_groups
                            .insert(expansion_key);
                    }
                    if let Some(taskmanager_shell::ProcessRowKey::Application(root_pid)) = row_key {
                        let _ = self.shell.select_application_row(root_pid, flat_index);
                    }
                }
            }
            ProjectedRow::Tree {
                pid,
                has_children,
                collapsed,
                parent_pid,
                ..
            } if has_children => {
                if expand && collapsed {
                    self.process_presentation.expanded_tree.remove(&pid);
                    let _ = self.shell.select_row(flat_index);
                } else if !expand && !collapsed {
                    self.process_presentation.expanded_tree.insert(pid);
                    let _ = self.shell.select_row(flat_index);
                } else if !expand && collapsed {
                    // Left on a collapsed node moves the cursor to its parent
                    // (the subtree is already hidden; same rule as the GPUI
                    // structural-arrow fold).
                    if let Some(index) = parent_pid
                        .and_then(|parent| self.shell.visible_process_index_of_pid(parent))
                    {
                        let _ = self.shell.select_row(index);
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn close_local_modals(&mut self) {
        self.dismiss_local_surface();
    }

    /// Close the shell-owned informational overlays before opening a local
    /// modal, so only one modal is visible and focused at a time.
    pub(super) fn close_shell_modals(&mut self) {
        self.shell.dismiss_informational_overlay();
        self.shell.close_service_log();
        self.shell.dismiss_overlay();
    }

    /// True when any renderer-local modal is open.
    pub(super) fn local_modal_open(&self) -> bool {
        self.local_surface_kind().is_some()
    }

    pub(super) fn modal_open(&self) -> bool {
        self.input_scope().modal_open()
    }

    pub(super) fn selected_table_focus_target(&self) -> Option<FocusTarget> {
        let page = self.shell.page();
        self.shell
            .table_row_count()
            .filter(|row_count| self.shell.selected < *row_count)
            .map(|_| FocusTarget::TableRow {
                page,
                index: self.shell.selected,
            })
    }

    /// Reveal a keyboard-selected row before the focus operation runs.
    /// Virtualization intentionally omits offscreen row widgets, so every
    /// scrollable table uses the same reveal path before focus is requested.
    pub(super) fn reveal_selected_table_row(&mut self) -> iced::Task<Message> {
        let page = self.shell.page();
        let row_index = if page == AppPage::Applications {
            self.process_presentation.visual_cursor
        } else {
            self.shell.selected
        };
        let window_height = self.viewport.size().height;
        let compact = self.compact_density();
        let (region, row_height) = match page {
            AppPage::Applications => (
                ViewportRegion::Applications,
                crate::ui::applications::application_row_height(compact),
            ),
            AppPage::Services => (
                ViewportRegion::Services,
                crate::ui::tables::inventory_row_height(compact),
            ),
            AppPage::Startup => (
                ViewportRegion::Startup,
                crate::ui::tables::inventory_row_height(compact),
            ),
            AppPage::Users => (
                ViewportRegion::Users,
                crate::ui::tables::inventory_row_height(compact),
            ),
            AppPage::AppHistory => (
                ViewportRegion::AppHistory,
                crate::ui::tables::inventory_row_height(compact),
            ),
            _ => return iced::Task::none(),
        };
        let state = self.viewport.scroll_mut(region);
        let viewport_height = state.viewport_height(window_height);
        // Sticky-header tables keep their header OUTSIDE the body scrollable,
        // so the keyboard reveal reserves no prefix; App-history's header
        // still scrolls in flow and keeps the header prefix.
        let reveal_prefix = match page {
            AppPage::AppHistory => crate::ui::VIRTUAL_TABLE_HEADER_HEIGHT,
            _ => 0.0,
        };
        reveal_row(state, row_index, row_height, reveal_prefix, viewport_height)
    }
}

fn reveal_row(
    state: &mut VirtualScrollState,
    row_index: usize,
    row_height: f32,
    header_height: f32,
    viewport_height: f32,
) -> iced::Task<Message> {
    let Some(target) =
        state.ensure_row_visible(row_index, row_height, header_height, viewport_height)
    else {
        return iced::Task::none();
    };

    state.set_offset(target);
    iced::widget::operation::scroll_to(
        state.id(),
        iced::widget::operation::AbsoluteOffset {
            x: None,
            y: Some(target),
        },
    )
}
