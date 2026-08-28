//! Direct-track semantic process-row selection and range operations.

use super::*;

impl ProcessRowKey {
    #[must_use]
    pub const fn process_pid(self) -> Option<u32> {
        match self {
            Self::Process(pid) => Some(pid),
            Self::Category(_) | Self::Application(_) => None,
        }
    }

    #[must_use]
    pub const fn application_root(self) -> Option<u32> {
        match self {
            Self::Application(pid) => Some(pid),
            Self::Category(_) | Self::Process(_) => None,
        }
    }
}

impl ProcessSelection {
    /// Plain click / context-menu focus: collapse to exactly one pid.
    pub fn select_single(&mut self, pid: u32) {
        self.pids.clear();
        self.pids.insert(pid);
        self.anchor = Some(pid);
        self.active_row = Some(ProcessRowKey::Process(pid));
    }

    /// Select a PID-less application aggregate without fabricating a
    /// representative process. Individual multi-selection is cleared; action
    /// resolution expands the root against the live tree at submit time.
    pub fn select_application(&mut self, root_pid: u32) {
        self.pids.clear();
        self.anchor = None;
        self.active_row = Some(ProcessRowKey::Application(root_pid));
    }

    /// Ctrl-click toggle: flip `pid` membership and make it the anchor when it
    /// stays a member; removing the anchor falls back to an arbitrary member
    /// (the set is never empty-checked here — removing the last member leaves
    /// an empty set with no anchor, which `batch_targets` reports honestly).
    pub fn toggle(&mut self, pid: u32) {
        if !self.pids.insert(pid) {
            self.pids.remove(&pid);
        }
        self.anchor = if self.pids.contains(&pid) {
            Some(pid)
        } else {
            self.pids.iter().next().copied()
        };
        self.active_row = self.anchor.map(ProcessRowKey::Process);
    }

    /// Shift-click range grow: insert every pid between the anchor and `end`
    /// (inclusive, in the caller's display order) and make `end` the anchor.
    /// A stale pid (not in the display order) inserts nothing — the direct
    /// track resolves rows against the live projection before calling.
    pub fn extend_range(&mut self, display_pids: &[u32], end: u32) {
        let anchor = self.anchor.unwrap_or(end);
        for pid in pid_range(display_pids, anchor, end) {
            self.pids.insert(pid);
        }
        self.anchor = Some(end);
        self.active_row = Some(ProcessRowKey::Process(end));
    }

    /// Keyboard navigation: move the anchor to `pid` and, unless `preserve_set`
    /// is set (Ctrl/Shift roaming), collapse the set to the new anchor.
    pub fn move_to(&mut self, pid: Option<u32>, preserve_set: bool) {
        self.anchor = pid;
        if !preserve_set {
            self.pids.clear();
        }
        if let Some(pid) = pid {
            self.pids.insert(pid);
        }
        self.active_row = pid.map(ProcessRowKey::Process);
    }

    /// Keyboard navigation over the renderer's semantic row order. Category
    /// headers are structural and therefore clear actionable selection;
    /// application/process rows become the active selectable row.
    pub fn move_to_row(&mut self, row: Option<ProcessRowKey>, preserve_set: bool) {
        match row {
            Some(ProcessRowKey::Process(pid)) => self.move_to(Some(pid), preserve_set),
            Some(ProcessRowKey::Application(root_pid)) => self.select_application(root_pid),
            Some(ProcessRowKey::Category(_)) | None => self.clear(),
        }
    }

    /// Replace the whole set and anchor in one step (capture scenarios and
    /// post-batch intent reconciliation that already know the exact targets).
    pub fn replace(&mut self, pids: impl IntoIterator<Item = u32>, anchor: Option<u32>) {
        self.pids = pids.into_iter().collect();
        self.anchor = anchor;
        self.active_row = anchor.map(ProcessRowKey::Process);
    }

    /// Drop every selected pid that no longer appears in the live process
    /// list; a dead anchor clears (mirrors the historical prune path — the
    /// anchor never silently jumps to a different process).
    pub fn retain_live(&mut self, live: &HashSet<u32>) {
        self.pids.retain(|pid| live.contains(pid));
        if self.anchor.is_some_and(|pid| !live.contains(&pid)) {
            self.anchor = None;
        }
        if self.active_row.is_some_and(|row| match row {
            ProcessRowKey::Application(root) | ProcessRowKey::Process(root) => {
                !live.contains(&root)
            }
            ProcessRowKey::Category(_) => false,
        }) {
            self.active_row = None;
        }
    }

    /// Clear both the set and the anchor.
    pub fn clear(&mut self) {
        self.pids.clear();
        self.anchor = None;
        self.active_row = None;
    }

    /// The authoritative multi-select set (renderers highlight members;
    /// batch intents freeze from this).
    #[must_use]
    pub fn pids(&self) -> &HashSet<u32> {
        &self.pids
    }

    /// The anchor pid (keyboard / plain-click focus; single-select identity).
    #[must_use]
    pub const fn anchor(&self) -> Option<u32> {
        self.anchor
    }

    /// The one row carrying keyboard focus / the primary selection surface.
    #[must_use]
    pub const fn active_row(&self) -> Option<ProcessRowKey> {
        self.active_row
    }

    #[must_use]
    pub const fn application_root(&self) -> Option<u32> {
        match self.active_row {
            Some(ProcessRowKey::Application(root)) => Some(root),
            Some(ProcessRowKey::Category(_) | ProcessRowKey::Process(_)) | None => None,
        }
    }

    /// Whether `pid` is part of the selection set.
    #[must_use]
    pub fn contains(&self, pid: u32) -> bool {
        self.pids.contains(&pid)
    }

    /// Batch-control targets: the sorted set when non-empty, otherwise the
    /// anchor as a single-target fallback — the same freeze input
    /// `ShellApp::request_process_batch` uses.
    #[must_use]
    pub fn batch_targets(&self) -> Vec<u32> {
        if self.pids.is_empty() {
            return self.anchor.into_iter().collect();
        }
        let mut pids: Vec<u32> = self.pids.iter().copied().collect();
        pids.sort_unstable();
        pids
    }
}
