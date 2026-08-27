//! Applications category-tree selection resolution.
//!
//! The category tree interleaves headers and indented process rows, so the
//! cursor ranges over a visual list wider than the flat process vector. This
//! module owns every projection and selection-resolution method that uses
//! that list: the visual row count, the hierarchy row snapshots, the
//! visual cursor motion, the aggregate/process expansion toggles, the selected-row
//! → process resolver, and the deduped process-insights re-request. Extracted
//! from `lib.rs` to keep the crate root under the source line budget
//! (behavior unchanged — every method stays reachable on `TuiApp`, impl
//! blocks may live in any module of the defining crate).

use taskmanager_application::{AppPage, FrozenProcessIdentity, PlatformEffect, ProcessItem};
use taskmanager_shell::ProcessRowKey;

use crate::TuiApp;
use crate::process_view;

impl TuiApp {
    /// Translate the shell's flat Applications anchor into the canonical
    /// category-tree cursor when the page gains focus. The shared shell keeps
    /// the selected process identity; the terminal cursor addresses visual
    /// rows that also include category and application aggregates.
    pub(crate) fn reconcile_applications_cursor(&mut self) {
        if self.page() != AppPage::Applications {
            return;
        }
        let target_pid = self
            .application
            .selected_process
            .as_ref()
            .map(|identity| identity.pid)
            .or_else(|| {
                self.visible_processes()
                    .get(self.selected)
                    .map(|process| process.pid)
            });
        let rows = self.process_rows_snapshot();
        let selected = target_pid
            .and_then(|pid| {
                rows.iter().position(|row| {
                    matches!(row, process_view::ProcessRow::TreeNode { process, .. } if process.pid == pid)
                })
            })
            .or_else(|| {
                target_pid.and_then(|pid| {
                    rows.iter().position(|row| {
                        matches!(row, process_view::ProcessRow::Group {
                            row_key: Some(ProcessRowKey::Application(root_pid)), ..
                        } if *root_pid == pid)
                    })
                })
            })
            .or_else(|| rows.iter().position(|row| match row {
                process_view::ProcessRow::TreeNode { .. } => true,
                process_view::ProcessRow::Group { row_key, .. } => row_key.is_some(),
            }))
            .unwrap_or(0);
        let process = process_view::process_at(&rows, selected).cloned();
        let row_key = process_view::row_key_at(&rows, selected);
        let _ = self.apply_selection_resolution_with_row(selected, process, row_key);
    }

    /// The number of visual rows in the canonical category hierarchy.
    #[must_use]
    pub(crate) fn visual_row_count(&self) -> usize {
        self.process_rows_snapshot().len()
    }

    /// Build one category-tree snapshot so navigation reuses one pure projection
    /// across the selection methods instead of rebuilding per call
    /// (`build_process_rows` is verified side-effect-free at `process_view.rs`).
    /// Returned rows borrow only the shared process vector.
    #[must_use]
    pub(crate) fn process_rows_snapshot(&self) -> Vec<process_view::ProcessRow<'_>> {
        let visible = self.visible_processes();
        process_view::build_process_rows(
            &visible,
            &self.expanded_groups,
            &self.collapsed_tree,
            self.process_sort,
        )
    }

    /// Clamp a cursor against the visual row count.
    fn clamp_cursor(selected: usize, delta: isize, count: usize) -> usize {
        if count == 0 {
            0
        } else {
            selected.saturating_add_signed(delta).min(count - 1)
        }
    }

    /// Cursor motion built once per key event: build the visual rows once,
    /// resolve the clamped cursor + owned selected
    /// process under the shared borrow, then mutate via
    /// [`Self::apply_selection_resolution`]. This is the per-frame reuse path —
    /// one `build_process_rows` instead of rebuilding for move, sync and
    /// insights. The scoped row borrow is dropped before mutation.
    pub(crate) fn move_nonflat_selection_oneshot(
        &mut self,
        delta: isize,
    ) -> Option<PlatformEffect> {
        let rows = self.process_rows_snapshot();
        let new_selected = Self::clamp_cursor(self.selected, delta, rows.len());
        let process = process_view::process_at(&rows, new_selected).cloned();
        let row_key = process_view::row_key_at(&rows, new_selected);
        let effect = self.apply_selection_resolution_with_row(new_selected, process, row_key);
        if let Some(ProcessRowKey::Process(pid)) = row_key {
            self.shell.selected_pids.insert(pid);
        }
        effect
    }

    /// Apply a resolved visual row including its semantic identity. An
    /// application aggregate clears PID multi-selection and stays PID-less;
    /// process rows retain their real identity; structural headers clear only
    /// the actionable anchor.
    pub(crate) fn apply_selection_resolution_with_row(
        &mut self,
        new_selected: usize,
        process: Option<ProcessItem>,
        row_key: Option<ProcessRowKey>,
    ) -> Option<PlatformEffect> {
        // A selection move lands on different content; the inline detail-panel
        // scroll offset is reset so a stale position from the previous row does
        // not survive into the new row's detail/insights cards.
        if self.selected != new_selected {
            self.detail_scroll_reset();
        }
        self.selected = new_selected;
        self.shell.selected_process_row = if self.page() == AppPage::Applications {
            row_key
        } else {
            None
        };
        self.shell.selected_pids.clear();
        let identity = process
            .as_ref()
            .and_then(FrozenProcessIdentity::from_process);
        self.application.selected_process = if self.page() == AppPage::Applications {
            identity
        } else {
            None
        };
        self.refresh_selected_process_insights_with(process)
    }

    /// Expand a resolved Tree node pid: remove it from the collapsed set and
    /// re-sync the application selection. The mutate tail for the reuse path
    /// (the resolver `tree_children_at` already confirmed the node has children).
    pub(crate) fn expand_tree_pid(&mut self, pid: u32) -> bool {
        self.collapsed_tree.remove(&pid);
        self.sync_grouped_application_selection();
        true
    }

    /// Collapse a resolved Tree node pid: insert it, re-clamp the cursor (a
    /// collapse removes rows below the node), and re-sync. The mutate tail for
    /// the reuse path.
    pub(crate) fn collapse_tree_pid(&mut self, pid: u32) -> bool {
        self.collapsed_tree.insert(pid);
        self.selected = self.selected.min(self.visual_row_count().saturating_sub(1));
        self.sync_grouped_application_selection();
        true
    }

    /// Toggle a resolved group header: flip its membership in
    /// [`Self::expanded_groups`] and re-clamp the cursor (expanding adds member
    /// rows below the header; collapsing removes them). The mutate tail for the
    /// reuse path. Always consumes the key once a header name was resolved.
    pub(crate) fn toggle_group_named(&mut self, group_name: String) -> bool {
        // `HashSet::insert` returns false when the name was already present.
        if !self.expanded_groups.insert(group_name.clone()) {
            self.expanded_groups.remove(&group_name);
        }
        self.selected = self.selected.min(self.visual_row_count().saturating_sub(1));
        self.sync_grouped_application_selection();
        true
    }

    /// Prune the TUI-local per-pid tree state against the live process set —
    /// the frontend-local equivalent of the shell's
    /// [`ShellApp::prune_stale_selection`], hung on the same "process domain
    /// changed" timing. `collapsed_tree` entries and
    /// `app-tree:<pid>` expansion keys whose pid exited are dropped, so a
    /// reused pid cannot inherit a stale collapse state; category expansion
    /// keys carry no pid and survive untouched.
    pub(crate) fn prune_stale_tree_state(&mut self) {
        let live: std::collections::HashSet<u32> = self
            .shell
            .projection()
            .processes
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|process| process.pid)
            .collect();
        self.collapsed_tree.retain(|pid| live.contains(pid));
        self.expanded_groups.retain(|key| {
            match key.strip_prefix(process_view::APP_TREE_EXPANSION_KEY_PREFIX) {
                // A malformed app-tree tail can never be regenerated by the
                // projection (pids are numeric), so dropping it is honest.
                Some(pid) => pid.parse::<u32>().is_ok_and(|pid| live.contains(&pid)),
                None => true,
            }
        });
    }

    /// Resolve the selected Applications row to the single process the details
    /// panel should show. On a group header there is no single process, so the panel
    /// renders its honest empty state. Owned so it can outlive the borrow on
    /// the shell's process vector.
    ///
    /// Delegates to the slice resolver so the single-build reuse path and this
    /// convenience accessor share one resolution path.
    #[must_use]
    pub(crate) fn selected_detail_process(&self) -> Option<ProcessItem> {
        let visible = self.visible_processes();
        if visible.is_empty() {
            return None;
        }
        let rows = self.process_rows_snapshot();
        self.selected_detail_process_rows(&rows)
    }

    /// Resolve the selected process from a prebuilt category-tree row slice.
    /// Read-only: the slice and `&self` are both shared borrows, so the caller may build the slice once
    /// and feed it to this resolver plus the cursor-motion path.
    #[must_use]
    pub(crate) fn selected_detail_process_rows(
        &self,
        rows: &[process_view::ProcessRow<'_>],
    ) -> Option<ProcessItem> {
        process_view::process_at(rows, self.selected).cloned()
    }

    /// Re-sync the shared application selection from the cursor's current
    /// visual row. The shell's `sync_application_selection` maps the cursor
    /// directly onto `visible_processes()[selected]`, which is wrong in the
    /// grouped and tree modes (the cursor indexes the interleaved visual
    /// list). A group header has no single process, so it clears the selection
    /// honestly rather than letting a destructive action target a stale/wrong
    /// PID.
    pub(crate) fn sync_grouped_application_selection(&mut self) {
        let (identity, row_key) = {
            let visible = self.visible_processes();
            if visible.is_empty() {
                (None, None)
            } else {
                let rows = self.process_rows_snapshot();
                let (process, row_key) = (
                    self.selected_detail_process_rows(&rows),
                    process_view::row_key_at(&rows, self.selected),
                );
                (
                    process
                        .as_ref()
                        .and_then(FrozenProcessIdentity::from_process),
                    row_key,
                )
            }
        };
        self.shell.selected_process_row = if self.page() == AppPage::Applications {
            row_key
        } else {
            None
        };
        if matches!(row_key, Some(ProcessRowKey::Application(_))) {
            self.shell.selected_pids.clear();
        }
        self.application.selected_process = if self.page() == AppPage::Applications {
            identity
        } else {
            None
        };
    }

    /// Re-request process insights for the currently selected Applications
    /// row, producing the effect to queue (the runtime queues it; this method
    /// never touches the platform). Called on every path that changes the
    /// Applications selection (arrow/page keys, sort reset, search reset, mode
    /// cycle, tree expansion) and from the runtime tick after each refresh.
    ///
    /// The request is deduped on the frozen identity: `submit_process_insights`
    /// bumps the projection revision on every submission, so re-requesting an
    /// unchanged selection would restart an in-flight collection instead of
    /// letting it complete. The dedupe also prevents the TUI from re-requesting
    /// when a key did not actually move the cursor. A group header / empty
    /// list / untrustworthy row honestly returns `None`.
    #[must_use]
    pub(crate) fn refresh_selected_process_insights(&mut self) -> Option<PlatformEffect> {
        let process = if self.page() == AppPage::Applications {
            self.selected_detail_process()
        } else {
            None
        };
        self.refresh_selected_process_insights_with(process)
    }

    /// Re-request process insights for a pre-resolved selected process. Lets
    /// the per-frame reuse path ([`Self::move_nonflat_selection_oneshot]) feed
    /// the process resolved from the single row build instead of rebuilding
    /// inside. The request is deduped on the frozen identity (see
    /// [`Self::refresh_selected_process_insights`]).
    #[must_use]
    pub(crate) fn refresh_selected_process_insights_with(
        &mut self,
        process: Option<ProcessItem>,
    ) -> Option<PlatformEffect> {
        if self.page() != AppPage::Applications {
            self.last_insights_target = None;
            return None;
        }
        let Some(identity) = process
            .as_ref()
            .and_then(FrozenProcessIdentity::from_process)
        else {
            self.last_insights_target = None;
            return None;
        };
        if self.last_insights_target.as_ref() == Some(&identity) {
            return None;
        }
        self.last_insights_target = Some(identity.clone());
        Some(PlatformEffect::ProcessInsights(identity))
    }
}
