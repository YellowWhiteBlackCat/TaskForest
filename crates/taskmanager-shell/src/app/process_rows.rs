//! Renderer-neutral process-row identity and projection generation.
//!
//! These types are the small shared seam between the canonical process fold
//! and renderer-local layout. They intentionally carry no widget or toolkit
//! state. A process/application row is anchored by the provider-issued live
//! key; a category row is structural and has no process target. The projection
//! generation is a separate stale-geometry guard and must not be confused with
//! either the process start token or a dangerous frozen control identity.

use std::num::{NonZeroU32, NonZeroU64};

use taskmanager_core::core::process::{ProcessCategory, ProcessItem};
use taskmanager_core::core::process_telemetry::ProcessIdentity;

/// Validated process identity owned by the shell's row projection.
///
/// The core owns the provider fact [`ProcessIdentity`]. This wrapper owns the
/// renderer-facing invariant that a row anchor may only contain non-zero
/// identity components; it is intentionally not a second core identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessRowIdentity {
    pid: NonZeroU32,
    start_token: NonZeroU64,
}

impl ProcessRowIdentity {
    /// Validate a provider identity before it can become row state.
    #[must_use]
    pub const fn new(identity: ProcessIdentity) -> Option<Self> {
        Self::from_parts(identity.pid, identity.start_token)
    }

    /// Validate raw identity components at the shell boundary.
    #[must_use]
    pub const fn from_parts(pid: u32, start_token: u64) -> Option<Self> {
        let Some(pid) = NonZeroU32::new(pid) else {
            return None;
        };
        let Some(start_token) = NonZeroU64::new(start_token) else {
            return None;
        };
        Some(Self { pid, start_token })
    }

    /// Derive the row identity only from a currently observed process.
    #[must_use]
    pub const fn from_process(process: &ProcessItem) -> Option<Self> {
        let Some(start_token) = process.current_start_token() else {
            return None;
        };
        Self::from_parts(process.pid, start_token)
    }

    /// The validated provider identity carried by the row.
    #[must_use]
    pub const fn identity(self) -> ProcessIdentity {
        ProcessIdentity {
            pid: self.pid.get(),
            start_token: self.start_token.get(),
        }
    }

    /// The process id used only for live re-resolution.
    #[must_use]
    pub const fn pid(self) -> u32 {
        self.pid.get()
    }

    /// The provider-issued start token used to reject PID reuse.
    #[must_use]
    pub const fn start_token(self) -> u64 {
        self.start_token.get()
    }
}

/// Stable identity of a row in the canonical Applications hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessRowId {
    /// Structural category header; it never carries a process target.
    Category(ProcessCategory),
    /// PID-less aggregate anchored to the process-tree root's live identity.
    Application(ProcessRowIdentity),
    /// Individual process row anchored to its live identity.
    Process(ProcessRowIdentity),
}

impl ProcessRowId {
    /// A process row anchored to one currently observed process.
    #[must_use]
    pub fn from_process(process: &ProcessItem) -> Option<Self> {
        ProcessRowIdentity::from_process(process).map(Self::Process)
    }

    /// An application-aggregate row anchored to the tree root's live
    /// identity. The root item — not a representative member — owns the
    /// anchor.
    #[must_use]
    pub fn application_of(root: &ProcessItem) -> Option<Self> {
        ProcessRowIdentity::from_process(root).map(Self::Application)
    }

    /// The provider-issued live key when this row represents a process-backed
    /// object. Category headers return `None`.
    #[must_use]
    pub const fn live_key(self) -> Option<ProcessRowIdentity> {
        match self {
            Self::Category(_) => None,
            Self::Application(key) | Self::Process(key) => Some(key),
        }
    }

    /// The PID used only to re-resolve a live tree or process row.
    #[must_use]
    pub const fn process_pid(self) -> Option<u32> {
        match self.live_key() {
            Some(key) => Some(key.pid()),
            None => None,
        }
    }

    /// Whether this row is an individual process rather than a structural or
    /// aggregate row. Application rows need tree expansion before a dangerous
    /// exact target can be frozen.
    #[must_use]
    pub const fn is_process(self) -> bool {
        matches!(self, Self::Process(_))
    }
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
