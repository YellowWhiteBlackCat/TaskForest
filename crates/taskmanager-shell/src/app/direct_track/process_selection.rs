//! Direct-track semantic process-row selection and range operations.

use super::*;

impl ProcessRowId {
    #[must_use]
    pub const fn process_identity(self) -> Option<ProcessLiveKey> {
        match self {
            Self::Process(identity) => Some(identity),
            Self::Category(_) | Self::Application(_) => None,
        }
    }

    #[must_use]
    pub const fn application_root(self) -> Option<ProcessLiveKey> {
        match self {
            Self::Application(identity) => Some(identity),
            Self::Category(_) | Self::Process(_) => None,
        }
    }
}

impl ProcessSelection {
    /// Plain click / context-menu focus: collapse to exactly one row.
    pub fn select_single(&mut self, identity: ProcessLiveKey) {
        self.rows.clear();
        self.rows.insert(identity);
        self.anchor = Some(identity);
        self.active_row = Some(ProcessRowId::Process(identity));
    }

    /// Select a PID-less application aggregate without fabricating a
    /// representative process. Individual multi-selection is cleared; action
    /// resolution expands the root against the live tree at submit time.
    pub fn select_application(&mut self, root: ProcessLiveKey) {
        self.rows.clear();
        self.anchor = None;
        self.active_row = Some(ProcessRowId::Application(root));
    }

    /// Ctrl-click toggle: flip membership and make the row the anchor when it
    /// stays a member; removing the anchor falls back to an arbitrary member
    /// (removing the last member leaves an empty set with no anchor, which
    /// [`Self::batch_identities`] reports honestly).
    pub fn toggle(&mut self, identity: ProcessLiveKey) {
        if !self.rows.insert(identity) {
            self.rows.remove(&identity);
        }
        self.anchor = if self.rows.contains(&identity) {
            Some(identity)
        } else {
            self.rows.iter().copied().next()
        };
        self.active_row = self.anchor.map(ProcessRowId::Process);
    }

    /// Shift-click range grow: insert every row between the anchor and `end`
    /// (inclusive, in the caller's display order) and make `end` the anchor.
    /// A stale row (not in the display order) inserts nothing — the direct
    /// track resolves rows against the live projection before calling.
    pub fn extend_range(&mut self, display: &[ProcessLiveKey], end: ProcessLiveKey) {
        let anchor = self.anchor.unwrap_or(end);
        for identity in identity_range(display, anchor, end) {
            self.rows.insert(identity);
        }
        self.anchor = Some(end);
        self.active_row = Some(ProcessRowId::Process(end));
    }

    /// Keyboard navigation: move the anchor to `identity` and, unless
    /// `preserve_set` is set (Ctrl/Shift roaming), collapse the set to the
    /// new anchor.
    pub fn move_to(&mut self, identity: Option<ProcessLiveKey>, preserve_set: bool) {
        self.anchor = identity;
        if !preserve_set {
            self.rows.clear();
        }
        if let Some(identity) = identity {
            self.rows.insert(identity);
        }
        self.active_row = identity.map(ProcessRowId::Process);
    }

    /// Keyboard navigation over the renderer's semantic row order. Category
    /// headers are structural and therefore clear actionable selection;
    /// application/process rows become the active selectable row.
    pub fn move_to_row(&mut self, row: Option<ProcessRowId>, preserve_set: bool) {
        match row {
            Some(ProcessRowId::Process(identity)) => self.move_to(Some(identity), preserve_set),
            Some(ProcessRowId::Application(root)) => self.select_application(root),
            Some(ProcessRowId::Category(_)) | None => self.clear(),
        }
    }

    /// Replace the whole set and anchor in one step (capture scenarios and
    /// post-batch intent reconciliation that already know the exact targets).
    pub fn replace(
        &mut self,
        identities: impl IntoIterator<Item = ProcessLiveKey>,
        anchor: Option<ProcessLiveKey>,
    ) {
        self.rows = identities.into_iter().collect();
        self.anchor = anchor;
        self.active_row = anchor.map(ProcessRowId::Process);
    }

    /// Reconcile against the accepted process snapshot (CORE-01): a row
    /// survives only when its full live identity — pid AND provider start
    /// token — resolves. A pid reused by a new process does not match and is
    /// dropped; the selection never silently retargets onto the impostor.
    pub fn reconcile(&mut self, live: &[ProcessItem]) {
        let live_rows: HashSet<ProcessLiveKey> = live
            .iter()
            .filter_map(ProcessLiveKey::from_process)
            .collect();
        self.rows.retain(|identity| live_rows.contains(identity));
        if self
            .anchor
            .is_some_and(|identity| !live_rows.contains(&identity))
        {
            self.anchor = None;
        }
        if self.active_row.is_some_and(|row| match row {
            ProcessRowId::Application(root) | ProcessRowId::Process(root) => {
                !live_rows.contains(&root)
            }
            ProcessRowId::Category(_) => false,
        }) {
            self.active_row = None;
        }
    }

    /// Clear both the set and the anchor.
    pub fn clear(&mut self) {
        self.rows.clear();
        self.anchor = None;
        self.active_row = None;
    }

    /// The authoritative multi-select set (renderers highlight members;
    /// batch intents freeze from this).
    #[must_use]
    pub fn rows(&self) -> &HashSet<ProcessLiveKey> {
        &self.rows
    }

    /// The anchor identity (keyboard / plain-click focus; single-select
    /// identity).
    #[must_use]
    pub const fn anchor(&self) -> Option<ProcessLiveKey> {
        self.anchor
    }

    /// The one row carrying keyboard focus / the primary selection surface.
    #[must_use]
    pub const fn active_row(&self) -> Option<ProcessRowId> {
        self.active_row
    }

    #[must_use]
    pub const fn application_root(&self) -> Option<ProcessLiveKey> {
        match self.active_row {
            Some(ProcessRowId::Application(root)) => Some(root),
            Some(ProcessRowId::Category(_) | ProcessRowId::Process(_)) | None => None,
        }
    }

    /// Whether `identity` is part of the selection set.
    #[must_use]
    pub fn contains(&self, identity: ProcessLiveKey) -> bool {
        self.rows.contains(&identity)
    }

    /// Batch-control targets: the identity set (pid-major order) when
    /// non-empty, otherwise the anchor as a single-target fallback.
    #[must_use]
    pub fn batch_identities(&self) -> Vec<ProcessLiveKey> {
        if self.rows.is_empty() {
            return self.anchor.into_iter().collect();
        }
        let mut identities: Vec<ProcessLiveKey> = self.rows.iter().copied().collect();
        identities.sort_unstable();
        identities
    }

    /// Freeze the batch targets against the live snapshot. An identity that
    /// no longer resolves exactly (pid + start token) is excluded — a
    /// dangerous effect never targets a pid-reuse impostor.
    #[must_use]
    pub fn frozen_targets(&self, live: &[ProcessItem]) -> Vec<FrozenProcessIdentity> {
        self.batch_identities()
            .into_iter()
            .filter_map(|identity| {
                live.iter()
                    .find(|process| {
                        process.pid == identity.pid()
                            && process.current_start_token() == Some(identity.start_token())
                    })
                    .and_then(FrozenProcessIdentity::from_process)
            })
            .collect()
    }
}

/// The identity range spanning `anchor` → `end` (inclusive, in the caller's
/// display order). A missing endpoint yields an empty range (the caller
/// keeps its prior set); this is the `&[ProcessLiveKey]` counterpart of
/// [`super::super::selection::selected_rows_range`].
pub fn identity_range(
    display: &[ProcessLiveKey],
    anchor: ProcessLiveKey,
    end: ProcessLiveKey,
) -> Vec<ProcessLiveKey> {
    let start = display.iter().position(|identity| *identity == anchor);
    let end_pos = display.iter().position(|identity| *identity == end);
    match (start, end_pos) {
        (Some(start), Some(end_pos)) => display[start.min(end_pos)..=start.max(end_pos)].to_vec(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/shell_app_direct_track_selection.rs"]
mod tests;
