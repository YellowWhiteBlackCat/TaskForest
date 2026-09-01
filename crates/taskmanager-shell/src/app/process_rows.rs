//! Renderer-neutral process-row identity and projection generation.
//!
//! These types are the small shared seam between the canonical process fold
//! and renderer-local layout. They intentionally carry no widget or toolkit
//! state. A process/application row is anchored by the provider-issued live
//! key; a category row is structural and has no process target. The projection
//! generation is a separate stale-geometry guard and must not be confused with
//! either the process start token or a dangerous frozen control identity.

use taskmanager_core::core::process::{ProcessCategory, ProcessItem, ProcessLiveKey};

mod tree;

pub use tree::{
    APP_TREE_EXPANSION_KEY_PREFIX, ProcessRowAggregate, ProcessTreeRow, app_tree_expansion_key,
    app_tree_expansion_key_for_identity, project_process_tree_rows,
};

/// Stable identity of a row in the canonical Applications hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessRowId {
    /// Structural category header; it never carries a process target.
    Category(ProcessCategory),
    /// PID-less aggregate anchored to the process-tree root's live identity.
    Application(ProcessLiveKey),
    /// Individual process row anchored to its live identity.
    Process(ProcessLiveKey),
}

impl ProcessRowId {
    /// A process row anchored to one currently observed process.
    #[must_use]
    pub fn from_process(process: &ProcessItem) -> Option<Self> {
        ProcessLiveKey::from_process(process).map(Self::Process)
    }

    /// An application-aggregate row anchored to the tree root's live
    /// identity. The root item — not a representative member — owns the
    /// anchor.
    #[must_use]
    pub fn application_of(root: &ProcessItem) -> Option<Self> {
        ProcessLiveKey::from_process(root).map(Self::Application)
    }

    /// The provider-issued live key when this row represents a process-backed
    /// object. Category headers return `None`.
    #[must_use]
    pub const fn live_key(self) -> Option<ProcessLiveKey> {
        match self {
            Self::Category(_) => None,
            Self::Application(key) | Self::Process(key) => Some(key),
        }
    }

    /// Whether this row is an individual process rather than a structural or
    /// aggregate row. Application rows need tree expansion before a dangerous
    /// exact target can be frozen.
    #[must_use]
    pub const fn is_process(self) -> bool {
        matches!(self, Self::Process(_))
    }

    /// Stable semantic identity for a process-table row.
    #[must_use]
    pub fn stable_key(self) -> String {
        match self {
            Self::Category(category) => format!("category:{category:?}"),
            Self::Application(identity) => format!("application:{}", identity.stable_key()),
            Self::Process(identity) => format!("process:{}", identity.stable_key()),
        }
    }
}

/// Stable semantic identity for one process row, including its incarnation.
///
/// Rows without a current start token remain displayable, but their key is
/// explicitly marked `unknown`; it must not look like a reusable live key.
#[must_use]
pub fn process_semantic_key(process: &ProcessItem) -> String {
    ProcessLiveKey::from_process(process).map_or_else(
        || format!("process:pid:{}:unknown", process.pid),
        |key| format!("process:{}", key.stable_key()),
    )
}

/// Generation of one accepted process projection.
///
/// This is a stale-frame/geometry token. It advances when the owning shell
/// accepts a new process snapshot, while unrelated service or hardware
/// updates leave it untouched. It is intentionally distinct from a process's
/// provider start token and from the application request ids used by controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessProjectionGeneration(u64);

impl ProcessProjectionGeneration {
    /// The initial generation before a process snapshot has been accepted.
    pub const INITIAL: Self = Self(0);

    /// Wrap a stored process-domain revision without treating it as a PID or
    /// provider token.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The underlying revision for cache keys and event payloads.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advance monotonically, saturating at the representable maximum.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// A row identity tied to the projection generation that produced it.
///
/// Pointer events and other delayed renderer messages should carry this
/// value. The identity can be used to retain a selection across a reorder;
/// the generation lets the receiver reject geometry from an older frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessRowAnchor {
    id: ProcessRowId,
    generation: ProcessProjectionGeneration,
}

impl ProcessRowAnchor {
    /// Create an anchor from a canonical row id and its projection generation.
    #[must_use]
    pub const fn new(id: ProcessRowId, generation: ProcessProjectionGeneration) -> Self {
        Self { id, generation }
    }

    /// The stable row identity.
    #[must_use]
    pub const fn id(self) -> ProcessRowId {
        self.id
    }

    /// The generation that owns the row geometry.
    #[must_use]
    pub const fn generation(self) -> ProcessProjectionGeneration {
        self.generation
    }

    /// Accept an event only when it belongs to the currently committed
    /// projection generation.
    #[must_use]
    pub fn belongs_to(self, generation: ProcessProjectionGeneration) -> bool {
        self.generation == generation
    }
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_process_rows.rs"]
mod tests;
