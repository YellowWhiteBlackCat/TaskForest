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

use std::collections::HashSet;

use taskmanager_application::{AppPage, PlatformEffect};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessItem, ProcessLiveKey};
use taskmanager_core::core::startup::StartupEntryId;
use taskmanager_core::core::target::{ServiceId, SessionId};
use taskmanager_shell::{
    APP_TREE_EXPANSION_KEY_PREFIX, InfoTable, ProcessRowId, ProcessTreeRow,
    project_process_tree_rows,
};

use crate::TuiApp;
use crate::process_view;

#[cfg(test)]
#[path = "../tests/headless/selection_support.rs"]
pub(crate) mod selection_support;

/// The Applications cursor's stable anchor. Structural category rows use
/// their locale-neutral expansion key; actionable process/application rows
/// additionally require the provider start-token so a PID reuse cannot retain
/// the old selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ApplicationRowAnchor {
    Structural(String),
    Actionable { key: ProcessRowId, start_token: u64 },
}

/// Stable identity for the three flat inventory tables.  The cursor remains
/// an index for rendering, but refresh/sort reconciliation uses the provider
/// identity instead of assuming row order is durable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InventoryRowAnchor {
    Service(ServiceId),
    Startup(StartupEntryId),
    Session(SessionId),
}

/// Cross-page selection anchor. The shell owns ONE `selected` index for every
/// table page, so a page A → B → A round trip cannot by itself know which row
/// A had selected — and batches may reorder or shrink A's rows while the page
/// is hidden. When a navigation leaves a table page, the selected row's
/// identity is captured under that page; when the page regains focus, the
/// identity is restored through the same page-local reconcile the refresh and
/// sort paths use (identity wins, vanished identity falls back to the clamped
/// cursor). The variants deliberately reuse the in-page anchors so a round
/// trip and a refresh share one matching rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PageRowAnchor {
    Application(ApplicationRowAnchor),
    Inventory(InventoryRowAnchor),
}

/// Small, owned cache key for the Applications shared-row projection. It
/// contains only presentation inputs, never borrowed process facts, so the
/// cache cannot outlive or redefine the shell projection. Every input that
/// can change the row slice's shape or content is part of the key: the
/// process revision (data), query and status filter (which rows are visible),
/// sort (visible order), and the expand/collapse sets (tree shape).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VisualRowCountKey {
    process_revision: u64,
    query: String,
    status_filter: taskmanager_shell::ProcessStatusFilter,
    sort: (taskmanager_shell::SortCol, taskmanager_shell::SortDir),
    expanded_groups: Vec<String>,
    collapsed_tree: Vec<ProcessLiveKey>,
}

/// The TUI's one presentation cache for the Applications shared-row
/// projection (TUI-006). On a key hit it serves BOTH the visual row count and
/// the fully-owned [`ProcessTreeRow`] slice, so the per-frame
/// cost of the Applications page follows the visible window instead of the
/// whole O(N) tree rebuild. The stored ids borrow nothing from the process
/// data; the key pins the exact visible list they index into.
#[derive(Clone, Debug)]
pub(crate) struct VisualRowCountCache {
    key: VisualRowCountKey,
    count: usize,
    rows: Vec<ProcessTreeRow>,
}

impl TuiApp {
    /// Capture the active flat inventory row before a refresh or sort changes
    /// its order. Empty provider identities are deliberately not anchors;
    /// those rows can still be shown, but they must use positional fallback
    /// rather than an invented identity.
    #[must_use]
    pub(crate) fn selected_inventory_row_anchor(&self) -> Option<InventoryRowAnchor> {
        match self.page() {
            AppPage::Services => self
                .sorted_service_at(self.selected)
                .filter(|service| !service.id.as_str().is_empty())
                .map(|service| InventoryRowAnchor::Service(service.id.clone())),
            AppPage::Startup => self
                .sorted_startup_entry_at(self.selected)
                .filter(|entry| !entry.id.is_empty())
                .map(|entry| InventoryRowAnchor::Startup(entry.id.clone())),
            AppPage::Users => self
                .sorted_session_at(self.selected)
                .filter(|session| !session.id.as_str().is_empty())
                .map(|session| InventoryRowAnchor::Session(session.id.clone())),
            _ => None,
        }
    }

    /// Reconcile the active flat inventory cursor after a provider batch or
    /// ordering change. Stable identity wins; if the target disappeared (or
    /// had no trustworthy identity), the old numeric cursor is clamped as the
    /// deterministic nearest-neighbor fallback.
    pub(crate) fn reconcile_inventory_row_anchor(&mut self, anchor: Option<InventoryRowAnchor>) {
        let target = match anchor.as_ref() {
            Some(InventoryRowAnchor::Service(id)) => self
                .sorted_services()
                .iter()
                .position(|service| service.id == *id),
            Some(InventoryRowAnchor::Startup(id)) => self
                .sorted_startup_entries()
                .iter()
                .position(|entry| entry.id == *id),
            Some(InventoryRowAnchor::Session(id)) => self
                .sorted_sessions()
                .iter()
                .position(|session| session.id == *id),
            None => None,
        }
        .unwrap_or_else(|| {
            let count = match self.page() {
                AppPage::Services => self.sorted_services().len(),
                AppPage::Startup => self.sorted_startup_entries().len(),
                AppPage::Users => self.sorted_sessions().len(),
                _ => 0,
            };
            self.selected.min(count.saturating_sub(1))
        });
        self.selected = target;
    }

    /// Preserve the selected inventory identity while the shared shell
    /// changes one table's sort column.
    pub(crate) fn cycle_info_sort_column_preserving_anchor(&mut self, table: InfoTable) {
        let anchor = self.selected_inventory_row_anchor();
        self.shell.cycle_info_sort_column(table);
        self.reconcile_inventory_row_anchor(anchor);
    }

    /// Preserve the selected inventory identity while reversing one table's
    /// sort direction.
    pub(crate) fn toggle_info_sort_direction_preserving_anchor(&mut self, table: InfoTable) {
        let anchor = self.selected_inventory_row_anchor();
        self.shell.toggle_info_sort_direction(table);
        self.reconcile_inventory_row_anchor(anchor);
    }

    /// Capture the selected row's identity of the page a navigation is about
    /// to leave. The cursor-less pages (Performance/System/AppHistory) and
    /// rows without a trustworthy provider identity capture nothing: a bare
    /// positional cursor is never promoted to an invented identity, so the
    /// restore path keeps the existing behavior for them.
    #[must_use]
    pub(crate) fn capture_page_row_anchor(&self) -> Option<PageRowAnchor> {
        match self.page() {
            AppPage::Applications => self
                .selected_application_row_anchor()
                .map(PageRowAnchor::Application),
            AppPage::Services | AppPage::Startup | AppPage::Users => self
                .selected_inventory_row_anchor()
                .map(PageRowAnchor::Inventory),
            AppPage::Performance | AppPage::System | AppPage::AppHistory => None,
        }
    }

    /// File the left page's captured anchor under that page, replacing any
    /// older anchor for it. An identity-less capture also forgets a stale
    /// previous anchor: the cursor no longer sits on the row that anchor
    /// described, so restoring it on return would move someone else's cursor.
    pub(crate) fn remember_page_row_anchor(
        &mut self,
        page: AppPage,
        anchor: Option<PageRowAnchor>,
    ) {
        match anchor {
            Some(anchor) => {
                self.page_row_anchors.insert(page, anchor);
            }
            None => {
                self.page_row_anchors.remove(&page);
            }
        }
    }

    /// Restore the entering page's stored anchor through that page's existing
    /// reconcile: the captured identity wins, a vanished identity falls back
    /// to that page's own deterministic entry fallback (the clamped cursor for
    /// the flat tables, the first actionable row for the category tree).
    /// Returns `true` when an anchor existed and the caller may skip the
    /// generic cursor re-derivation — the anchor is strictly more faithful,
    /// because other pages and refresh waves legitimately clear the shell's
    /// derived process selection while this page was hidden. `false` leaves
    /// the existing entry behavior fully in charge.
    pub(crate) fn restore_page_row_anchor(&mut self) -> bool {
        let Some(anchor) = self.page_row_anchors.remove(&self.page()) else {
            return false;
        };
        match anchor {
            PageRowAnchor::Application(anchor) => {
                let matched = self.with_canonical_rows_indexed(|ids, visible| {
                    ids.iter()
                        .any(|row| self.application_row_id_matches(row, visible, &anchor))
                });
                self.reconcile_application_row_anchor(Some(anchor));
                if !matched {
                    // The captured identity left the projection while the page
                    // was hidden. The bare clamp could land on a structural
                    // header, so the tree falls back to its own entry rule:
                    // the first actionable row (or row zero when none exists).
                    self.reconcile_applications_cursor();
                }
            }
            PageRowAnchor::Inventory(anchor) => {
                self.reconcile_inventory_row_anchor(Some(anchor));
            }
        }
        true
    }

    /// Reconcile the Performance page's frontend-local resource selection
    /// after the projection changed. The selection's stable identity is its
    /// locale-neutral resource token (`PerfDevice::label_key`); the projection
    /// backs it with the device facts `visible_perf_devices` gates on, so a
    /// hot-unplug (or a provider going dark) fails closed: the selection falls
    /// back to the first resource the projection still backs — never a family
    /// whose facts vanished, and never an invented device. When no resource is
    /// visible at all, the raw token is kept as the explicit empty state: the
    /// panels render honest absence for empty facts and nothing is fabricated
    /// as zero or success.
    pub(crate) fn reconcile_perf_device_anchor(&mut self) {
        let visible = self.visible_perf_devices();
        if visible.contains(&self.perf_device) {
            return;
        }
        if let Some(first) = visible.first().copied() {
            // `select_perf_device` also drops the resource-local viewport
            // intent, so the fallback cannot inherit the vanished device's
            // scroll position.
            self.select_perf_device(first);
        }
    }

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
                self.shell
                    .visible_process_at(self.selected)
                    .map(|process| process.pid)
            });
        let (selected, process, row_key) = self.with_canonical_rows_indexed(|ids, visible| {
            let selected = target_pid
                .and_then(|pid| {
                    ids.iter().position(|row| {
                        matches!(row, ProcessTreeRow::Process { .. }
                            if visible.process_of(row).is_some_and(|process| process.pid == pid))
                    })
                })
                .or_else(|| {
                    target_pid.and_then(|pid| {
                        let root_key = self
                            .process_start_token_for_pid(pid)
                            .and_then(|token| ProcessLiveKey::from_parts(pid, token))
                            .map(ProcessRowId::Application);
                        root_key.and_then(|key| {
                            ids.iter()
                                .position(|row| visible.row_key_of(row) == Some(key))
                        })
                    })
                })
                .or_else(|| ids.iter().position(|row| visible.row_key_of(row).is_some()))
                .unwrap_or(0);
            (
                selected,
                visible.id_process(ids, selected).cloned(),
                visible.id_row_key(ids, selected),
            )
        });
        let _ = self.apply_selection_resolution_with_row(selected, process, row_key);
    }

    /// The number of visual rows in the canonical category hierarchy.
    #[must_use]
    pub(crate) fn visual_row_count(&self) -> usize {
        let key = self.visual_row_count_key();
        if let Some(count) = self
            .visual_row_count_cache
            .borrow()
            .as_ref()
            .filter(|cache| cache.key == key)
            .map(|cache| cache.count)
        {
            return count;
        }
        let rows = self.build_canonical_rows();
        let count = rows.len();
        *self.visual_row_count_cache.borrow_mut() = Some(VisualRowCountCache { key, count, rows });
        count
    }

    /// Whether the presentation cache is warm for the CURRENT inputs (the
    /// stored key matches the live key). A behavioral probe of the
    /// hit/invalidation machinery: any of the five inputs (revision, query,
    /// status filter, sort, expand/collapse sets) changing must turn this
    /// false until the next rebuild.
    fn canonical_row_cache_is_valid(&self) -> bool {
        let key = self.visual_row_count_key();
        self.visual_row_count_cache
            .borrow()
            .as_ref()
            .is_some_and(|cache| cache.key == key)
    }

    /// Test-visible delegate of the cache-hit probe (see
    /// [`Self::canonical_row_cache_is_valid`]).
    /// The pure O(N) rebuild of the owned shared row slice from the live
    /// projection. The only producer of cache entries.
    fn build_canonical_rows(&self) -> Vec<ProcessTreeRow> {
        let visible = self.visible_processes();
        let observed_at_ms = self.shell.projection().processes_observed_at_ms;
        project_process_tree_rows(
            &visible,
            &self.expanded_groups,
            &self.collapsed_tree,
            self.process_sort,
            observed_at_ms,
        )
    }

    /// Read the cached owned shared row slice together with the visible
    /// process list it indexes into, rebuilding the cache first when the
    /// presentation key changed. This is the BORROWED-SLICE entry: the read
    /// closure receives a fully materialized `&[&ProcessItem]`, so it pays
    /// one O(visible N) pointer-vector allocation per call. Per-frame render
    /// consumers use [`Self::with_canonical_rows_indexed`] instead; this
    /// entry remains for full-row materialization consumers
    /// ([`Self::process_rows_snapshot`]) and the test-side canonical detail
    /// entry, which both want whole-list borrowing anyway.
    ///
    /// The read closure runs under the cache borrow and must not re-enter
    /// [`Self::visual_row_count`], [`Self::process_rows_snapshot`], this
    /// method, or any other cache producer — the single-borrow `RefCell`
    /// would fail the re-entrant access loudly rather than corrupt. Every
    /// mutation stays outside the closure; only reads happen inside.
    pub(crate) fn with_canonical_rows<'s, R>(
        &'s self,
        read: impl FnOnce(&[ProcessTreeRow], &[&'s ProcessItem]) -> R,
    ) -> R {
        self.ensure_canonical_row_cache();
        let cache = self.visual_row_count_cache.borrow();
        let visible = self.visible_processes();
        match cache.as_ref() {
            Some(cache) => read(&cache.rows, &visible),
            // The rebuild above always installs an entry, so an empty cell
            // here cannot happen; if it ever did, the honest read is the
            // empty projection — no rows are invented.
            None => read(&[], &[]),
        }
    }

    /// The indexed twin of [`Self::with_canonical_rows`] and THE per-frame
    /// path: the read closure receives the cached owned id slice plus a lazy
    /// [`process_view::VisibleProcesses`] accessor over the shell's memoized
    /// visible-row indices, so a consumer resolves `&ProcessItem` values ON
    /// DEMAND and touches only the visible window it paints. Unlike the
    /// borrowed-slice twin this entry never materializes the O(visible N)
    /// pointer vector — the only O(N) walk left is the cache-key-miss
    /// rebuild, which is the cache's job.
    ///
    /// The read closure runs under the cache borrow and must not re-enter
    /// any cache producer (see [`Self::with_canonical_rows`]); only reads
    /// happen inside.
    pub(crate) fn with_canonical_rows_indexed<'s, R>(
        &'s self,
        read: impl FnOnce(&[ProcessTreeRow], &process_view::VisibleProcesses<'s>) -> R,
    ) -> R {
        self.ensure_canonical_row_cache();
        let cache = self.visual_row_count_cache.borrow();
        // One memo probe per frame (Rc clone, no O(N) allocation): the
        // indices pin the exact visible ordering the cache key pins, so each
        // on-demand resolution lands on the process the ids were built from.
        let visible = process_view::VisibleProcesses::new(
            self.shell.visible_process_indices(),
            self.shell.projection().processes_slice(),
        );
        match cache.as_ref() {
            Some(cache) => read(&cache.rows, &visible),
            // The rebuild above always installs an entry, so an empty cell
            // here cannot happen; if it ever did, the honest read is the
            // empty projection — no rows are invented.
            None => read(&[], &visible),
        }
    }

    /// Ensure the presentation cache holds an entry for the CURRENT inputs,
    /// rebuilding it exactly once after a key change. The only producer of
    /// cache entries; both `with_canonical_rows*` entries share it.
    fn ensure_canonical_row_cache(&self) {
        if !self.canonical_row_cache_is_valid() {
            let key = self.visual_row_count_key();
            let rows = self.build_canonical_rows();
            let count = rows.len();
            *self.visual_row_count_cache.borrow_mut() =
                Some(VisualRowCountCache { key, count, rows });
        }
    }

    /// The presentation key behind the shared-row cache. Deliberately NOT
    /// self-cached: the two per-frame probes re-derive it on every call
    /// (~4-8 allocations — the sorted `expanded_groups` clones — roughly 1%
    /// of the measured steady-state frame in
    /// `tests/perf_budget_alloc_tests.rs`), and an allocation-free validity
    /// probe would add a second hand-rolled comparison path for a
    /// sub-threshold win. Every input below is checked for exact equality, so
    /// any change fails closed into a rebuild.
    fn visual_row_count_key(&self) -> VisualRowCountKey {
        let mut expanded_groups: Vec<String> = self.expanded_groups.iter().cloned().collect();
        expanded_groups.sort_unstable();
        let mut collapsed_tree: Vec<ProcessLiveKey> = self.collapsed_tree.iter().copied().collect();
        collapsed_tree.sort_unstable();
        VisualRowCountKey {
            process_revision: self.shell.projection().process_revision,
            query: self.query.trim().to_owned(),
            status_filter: self.process_status_filter,
            sort: self.process_sort,
            expanded_groups,
            collapsed_tree,
        }
    }

    /// Build one category-tree snapshot so navigation reuses one pure projection
    /// across the selection methods instead of rebuilding per call
    /// (`build_process_rows` is verified side-effect-free at `process_view.rs`).
    /// The rows are materialized from the cached owned id slice when the
    /// presentation key is unchanged, so this stays byte-identical to a fresh
    /// rebuild while skipping the O(N) tree walk. Returned rows borrow only
    /// the shared process vector.
    #[must_use]
    pub(crate) fn process_rows_snapshot(&self) -> Vec<process_view::ProcessRow<'_>> {
        self.with_canonical_rows(process_view::materialize_rows)
    }

    /// Clamp a cursor against the visual row count.
    fn clamp_cursor(selected: usize, delta: isize, count: usize) -> usize {
        if count == 0 {
            0
        } else {
            selected.saturating_add_signed(delta).min(count - 1)
        }
    }

    /// Cursor motion built once per key event: resolve the clamped cursor +
    /// owned selected process from the cached shared row slice under the
    /// shared borrow, then mutate via
    /// [`Self::apply_selection_resolution`]. This is the per-frame reuse path —
    /// O(1) id resolutions instead of a full row materialization. The scoped
    /// row borrow is dropped before mutation.
    pub(crate) fn move_nonflat_selection_oneshot(
        &mut self,
        delta: isize,
    ) -> Option<PlatformEffect> {
        let (new_selected, process, row_key) = self.with_canonical_rows_indexed(|ids, visible| {
            let new_selected = Self::clamp_cursor(self.selected, delta, ids.len());
            (
                new_selected,
                visible.id_process(ids, new_selected).cloned(),
                visible.id_row_key(ids, new_selected),
            )
        });
        let effect = self.apply_selection_resolution_with_row(new_selected, process, row_key);
        if let Some(ProcessRowId::Process(identity)) = row_key {
            self.shell.selected_rows.insert(identity);
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
        row_key: Option<ProcessRowId>,
    ) -> Option<PlatformEffect> {
        // A selection move lands on different content; the inline detail-panel
        // scroll offset is reset so a stale position from the previous row does
        // not survive into the new row's detail/insights cards.
        if self.selected != new_selected {
            self.detail_scroll_reset();
        }
        self.selected = new_selected;
        self.shell.selected_row = if self.page() == AppPage::Applications {
            row_key
        } else {
            None
        };
        self.shell.selected_rows.clear();
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
    pub(crate) fn expand_tree_identity(&mut self, identity: ProcessLiveKey) -> bool {
        let anchor = self.selected_application_row_anchor();
        self.collapsed_tree.remove(&identity);
        self.reconcile_application_row_anchor(anchor);
        true
    }

    /// Collapse a resolved Tree node pid: insert it, re-clamp the cursor (a
    /// collapse removes rows below the node), and re-sync. The mutate tail for
    /// the reuse path.
    pub(crate) fn collapse_tree_identity(&mut self, identity: ProcessLiveKey) -> bool {
        let anchor = self.selected_application_row_anchor();
        self.collapsed_tree.insert(identity);
        self.reconcile_application_row_anchor(anchor);
        true
    }

    /// Toggle a resolved group header: flip its membership in
    /// [`Self::expanded_groups`] and re-clamp the cursor (expanding adds member
    /// rows below the header; collapsing removes them). The mutate tail for the
    /// reuse path. Always consumes the key once a header name was resolved.
    pub(crate) fn toggle_group_named(&mut self, group_name: String) -> bool {
        let anchor = self.selected_application_row_anchor();
        // `HashSet::insert` returns false when the name was already present.
        if !self.expanded_groups.insert(group_name.clone()) {
            self.expanded_groups.remove(&group_name);
        }
        self.reconcile_application_row_anchor(anchor);
        true
    }

    /// Capture the currently selected Applications visual row before a data
    /// or ordering mutation. The returned value is intentionally absent when
    /// the row has no trustworthy provider identity; callers then use the
    /// deterministic positional fallback instead of guessing an identity.
    #[must_use]
    pub(crate) fn selected_application_row_anchor(&self) -> Option<ApplicationRowAnchor> {
        if self.page() != AppPage::Applications {
            return None;
        }
        self.with_canonical_rows_indexed(|ids, visible| {
            match ids.get(self.selected)? {
                ProcessTreeRow::Category { .. } => {
                    // A structural category header anchors on its
                    // locale-neutral expansion key; it has no pid.
                    visible
                        .expansion_key_of(ids.get(self.selected)?)
                        .map(ApplicationRowAnchor::Structural)
                }
                ProcessTreeRow::Application { .. } => {
                    let key = visible.row_key_of(ids.get(self.selected)?)?;
                    Some(ApplicationRowAnchor::Actionable {
                        key,
                        start_token: self.process_start_token_for_key(key)?,
                    })
                }
                ProcessTreeRow::Process { .. } => {
                    let process = visible.process_of(ids.get(self.selected)?)?;
                    Some(ApplicationRowAnchor::Actionable {
                        key: ProcessRowId::from_process(process)?,
                        start_token: process.current_start_token()?,
                    })
                }
            }
        })
    }

    /// Re-anchor the Applications cursor after the visual row projection has
    /// changed. The old row identity wins; if it disappeared, the old numeric
    /// position is clamped to the new row list as the nearest deterministic
    /// fallback. This method does not clear multi-selection marks.
    pub(crate) fn reconcile_application_row_anchor(
        &mut self,
        anchor: Option<ApplicationRowAnchor>,
    ) {
        if self.page() != AppPage::Applications {
            return;
        }
        let target = self.with_canonical_rows_indexed(|ids, visible| {
            anchor
                .as_ref()
                .and_then(|anchor| {
                    ids.iter()
                        .position(|row| self.application_row_id_matches(row, visible, anchor))
                })
                .unwrap_or_else(|| self.selected.min(ids.len().saturating_sub(1)))
        });
        if self.selected != target {
            self.detail_scroll_reset();
        }
        self.selected = target;
        self.sync_grouped_application_selection();
    }

    /// Preserve the selected Applications row while the shell changes its
    /// process sort. The shell still owns the sort and notice; this wrapper
    /// only restores the renderer-local visual anchor afterward.
    pub(crate) fn set_process_sort_column_preserving_anchor(
        &mut self,
        column: taskmanager_shell::SortCol,
    ) {
        let anchor = self.selected_application_row_anchor();
        self.shell.set_sort_column(column);
        self.reconcile_application_row_anchor(anchor);
    }

    /// Preserve the selected Applications row while reversing the shell's
    /// process sort direction.
    pub(crate) fn toggle_process_sort_direction_preserving_anchor(&mut self) {
        let anchor = self.selected_application_row_anchor();
        self.shell.toggle_sort_direction();
        self.reconcile_application_row_anchor(anchor);
    }

    /// Prune the TUI-local tree state against the live process set —
    /// the frontend-local equivalent of the shell's
    /// [`ShellApp::prune_stale_selection`], hung on the same "process domain
    /// changed" timing. `collapsed_tree` entries and
    /// `app-tree:<live-key>` expansion keys are validated against the current
    /// core-owned identities, so a reused pid cannot inherit stale expansion
    /// state; category expansion keys carry no process identity and survive
    /// untouched.
    pub(crate) fn prune_stale_tree_state(&mut self) {
        let current: HashSet<ProcessLiveKey> = self
            .shell
            .projection()
            .processes_slice()
            .iter()
            .filter_map(ProcessLiveKey::from_process)
            .collect();
        self.collapsed_tree
            .retain(|identity| current.contains(identity));
        self.expanded_groups.retain(|key| {
            key.strip_prefix(APP_TREE_EXPANSION_KEY_PREFIX)
                .is_none_or(|suffix| {
                    current
                        .iter()
                        .any(|identity| identity.stable_key() == suffix)
                })
        });
    }

    /// Resolve the selected Applications row to the single process the details
    /// panel should show. On a group header there is no single process, so the panel
    /// renders its honest empty state. Owned so it can outlive the borrow on
    /// the shell's process vector.
    ///
    /// Resolves through the cached shared row slice — O(1) on a cache hit.
    #[must_use]
    pub(crate) fn selected_detail_process(&self) -> Option<ProcessItem> {
        if self.shell.visible_process_count() == 0 {
            return None;
        }
        self.with_canonical_rows_indexed(|ids, visible| {
            visible.id_process(ids, self.selected).cloned()
        })
    }

    /// Resolve the selected process from a prebuilt category-tree row slice.
    /// Read-only: the slice and `&self` are both shared borrows, so the caller may build the slice once
    /// and feed it to this resolver plus the cursor-motion path. Retained as
    /// the materialized-row reference resolver the snapshot-parity tests
    /// compare the cached id path against.
    /// Re-sync the shared application selection from the cursor's current
    /// visual row. The shell's `sync_application_selection` maps the cursor
    /// directly onto `visible_processes()[selected]`, which is wrong in the
    /// grouped and tree modes (the cursor indexes the interleaved visual
    /// list). A group header has no single process, so it clears the selection
    /// honestly rather than letting a destructive action target a stale/wrong
    /// PID.
    pub(crate) fn sync_grouped_application_selection(&mut self) {
        let (identity, row_key) = self.with_canonical_rows_indexed(|ids, visible| {
            if visible.is_empty() {
                (None, None)
            } else {
                let process = visible.id_process(ids, self.selected).cloned();
                let row_key = visible.id_row_key(ids, self.selected);
                (
                    process
                        .as_ref()
                        .and_then(FrozenProcessIdentity::from_process),
                    row_key,
                )
            }
        });
        self.shell.selected_row = if self.page() == AppPage::Applications {
            row_key
        } else {
            None
        };
        if matches!(row_key, Some(ProcessRowId::Application(_))) {
            self.shell.selected_rows.clear();
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

    fn process_start_token_for_key(&self, key: ProcessRowId) -> Option<u64> {
        // The identity key already carries the provider start token
        // (CORE-01); structural rows have none.
        key.live_key().map(|identity| identity.start_token())
    }

    /// The current provider start token at one PID lookup hint, resolved
    /// against the accepted snapshot (used only by compatibility fallbacks).
    fn process_start_token_for_pid(&self, pid: u32) -> Option<u64> {
        self.shell
            .projection()
            .processes_slice()
            .iter()
            .find(|process| process.pid == pid)
            .and_then(ProcessItem::current_start_token)
    }

    /// Whether the canonical row with id `row` (resolved against the visible
    /// list the ids index) carries the anchor's identity. Structural category
    /// rows match their locale-neutral expansion key; actionable rows require
    /// the provider start-token so a PID reuse cannot retain the old
    /// selection. This is the owned-id counterpart of the materialized-row
    /// matching rule the anchors have always used.
    fn application_row_id_matches(
        &self,
        row: &ProcessTreeRow,
        visible: &process_view::VisibleProcesses<'_>,
        anchor: &ApplicationRowAnchor,
    ) -> bool {
        match (row, anchor) {
            (ProcessTreeRow::Category { .. }, ApplicationRowAnchor::Structural(anchor_key)) => {
                visible.expansion_key_of(row).as_deref() == Some(anchor_key.as_str())
            }
            (
                ProcessTreeRow::Application { .. },
                ApplicationRowAnchor::Actionable {
                    key: anchor_key,
                    start_token,
                },
            ) => {
                visible.row_key_of(row).as_ref() == Some(anchor_key)
                    && self.process_start_token_for_key(*anchor_key) == Some(*start_token)
            }
            (
                ProcessTreeRow::Process { .. },
                ApplicationRowAnchor::Actionable {
                    key: ProcessRowId::Process(anchor_identity),
                    start_token,
                },
            ) => {
                anchor_identity.start_token() == *start_token
                    && visible.process_of(row).is_some_and(|process| {
                        ProcessRowId::from_process(process)
                            .is_some_and(|key| key == ProcessRowId::Process(*anchor_identity))
                    })
            }
            _ => false,
        }
    }
}
