//! Platform-neutral process-control identity, intent, and result contracts.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use super::{ProcessItem, ProcessLiveKey};
use crate::core::FailureKind;

/// Platform-neutral scheduling priority tier (ARCH.md §8.1).
///
/// The UI vocabulary is High/Normal/Low; adapters map each tier to the
/// native primitive (Linux/macOS nice, Windows priority class). A raw nice
/// value or priority-class number never crosses the neutral model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PriorityTier {
    High,
    #[default]
    Normal,
    Low,
}

impl PriorityTier {
    pub const ALL: [Self; 3] = [Self::High, Self::Normal, Self::Low];

    /// Locale catalog key (`proc.high` / `proc.normal` / `proc.low`).
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::High => "proc.high",
            Self::Normal => "proc.normal",
            Self::Low => "proc.low",
        }
    }

    /// Canonical Linux/macOS nice value the presets have always sent.
    #[must_use]
    pub const fn canonical_nice(self) -> i32 {
        match self {
            Self::High => -10,
            Self::Normal => 0,
            Self::Low => 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessBatchAction {
    End,
    Kill,
    Suspend,
    Resume,
    SetPriority(PriorityTier),
}

/// Cross-platform semantic process signal.
///
/// A provider maps these intents to its native control primitive. Platforms
/// without a matching primitive report typed unsupported capability at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessSignal {
    Terminate,
    Kill,
    Stop,
    Continue,
    Hangup,
    Interrupt,
    User1,
    User2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenProcessIdentity {
    pub pid: u32,
    pub name: String,
    /// Schema-v1 display/export compatibility only. Never authorizes a read or
    /// mutation because wall-clock seconds are not provider-native identity.
    pub start_time_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_token: Option<NonZeroU64>,
}

impl FrozenProcessIdentity {
    /// Freeze a current row only when its exact provider-native identity is
    /// available. An old/default/stale row cannot create new authority.
    #[must_use]
    pub fn from_process(process: &ProcessItem) -> Option<Self> {
        Self::from_authoritative_parts(
            process.pid,
            process.name.clone(),
            process.current_start_time_secs().unwrap_or_default(),
            process.current_start_token()?,
        )
    }

    /// Shared constructor for native adapters and deterministic fixtures.
    ///
    /// Production frontends should prefer [`Self::from_process`].
    #[must_use]
    pub fn from_authoritative_parts(
        pid: u32,
        name: impl Into<String>,
        start_time_secs: u64,
        start_token: u64,
    ) -> Option<Self> {
        if pid == 0 {
            return None;
        }
        Some(Self {
            pid,
            name: name.into(),
            start_time_secs,
            start_token: Some(NonZeroU64::new(start_token)?),
        })
    }

    /// Exact token required by every provider read or mutation.
    ///
    /// `None` identifies a schema-v1 payload or invalid input and must fail
    /// closed. The legacy wall-clock field is never a substitute.
    #[must_use]
    pub const fn authoritative_start_token(&self) -> Option<u64> {
        match self.start_token {
            Some(token) if self.pid > 0 => Some(token.get()),
            Some(_) | None => None,
        }
    }
}

/// How a process-tree termination expands its target set (ARCH.md §8.1).
///
/// `PidAdjacency` freezes the descendant closure over `parent_pid` — the
/// universally available projection. `NativeGroup` names a provider-native
/// grouping (cgroup membership, job object, coalition) whose members the
/// adapter revalidates and merges into the frozen set; adapters that have no
/// verified native-group primitive for control simply never produce it
/// (mapping stays PidAdjacency — 映射穷尽律 does not require inventing reads).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProcessGroupScope {
    #[default]
    PidAdjacency,
    NativeGroup {
        /// Neutral grouping family the adapter verified (e.g. "cgroup.v2").
        family: String,
        /// Opaque native locator revalidated by the control path.
        locator: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessBatchIntent {
    pub action: ProcessBatchAction,
    /// Target-set expansion contract; defaults to `PidAdjacency` so legacy
    /// payloads and non-tree batches decode without native-group semantics.
    #[serde(default)]
    pub scope: ProcessGroupScope,
    pub targets: Vec<FrozenProcessIdentity>,
}

impl ProcessBatchIntent {
    #[must_use]
    pub fn freeze(
        processes: &[ProcessItem],
        selected_identities: impl IntoIterator<Item = ProcessLiveKey>,
        action: ProcessBatchAction,
    ) -> Self {
        let mut selected: Vec<_> = selected_identities.into_iter().collect();
        selected.sort_unstable();
        selected.dedup();
        Self {
            action,
            scope: ProcessGroupScope::PidAdjacency,
            targets: selected
                .into_iter()
                .filter_map(|identity| {
                    processes
                        .iter()
                        .find(|process| ProcessLiveKey::from_process(process) == Some(identity))
                })
                .filter_map(FrozenProcessIdentity::from_process)
                .collect(),
        }
    }

    /// Freeze one process and all currently-known descendants in deterministic
    /// leaf-to-root order. This is the shared semantic behind a frontend's
    /// “end process tree” menu item; the renderer owns the confirmation UI,
    /// while this contract owns identity capture and execution order. The
    /// frozen scope is [`ProcessGroupScope::PidAdjacency`] — the expansion is
    /// exactly the `parent_pid` closure known to the model today.
    #[must_use]
    pub fn freeze_tree(
        processes: &[ProcessItem],
        root: ProcessLiveKey,
        action: ProcessBatchAction,
    ) -> Self {
        Self {
            action,
            scope: ProcessGroupScope::PidAdjacency,
            targets: descendant_live_keys(processes, root)
                .into_iter()
                .filter_map(|identity| {
                    processes
                        .iter()
                        .find(|process| ProcessLiveKey::from_process(process) == Some(identity))
                })
                .filter_map(FrozenProcessIdentity::from_process)
                .collect(),
        }
    }
}

/// The ONE parent_pid tree traversal (同一律): the leaf-first live-identity
/// closure of `root` — every descendant, deepest first, ending with `root`
/// itself — shared by [`ProcessBatchIntent::freeze_tree`] and the frontends'
/// pre-freeze tree previews. Siblings are PID-sorted for deterministic order;
/// the returned values are still exact live identities, never PID targets.
/// The visited set makes the walk total on cyclic `parent_pid` chains. An
/// unknown or identity-less root yields an EMPTY vector (fail closed — a
/// control intent over a dead row has no honest targets).
#[must_use]
pub fn descendant_live_keys(
    processes: &[ProcessItem],
    root: ProcessLiveKey,
) -> Vec<ProcessLiveKey> {
    let Some(root_process) = processes
        .iter()
        .find(|process| ProcessLiveKey::from_process(process) == Some(root))
    else {
        return Vec::new();
    };
    let by_pid: HashMap<u32, &ProcessItem> = processes
        .iter()
        .map(|process| (process.pid, process))
        .collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        if let Some(parent) = process.parent_pid
            && process.pid != root.pid()
        {
            children.entry(parent).or_default().push(process.pid);
        }
    }
    for child_pids in children.values_mut() {
        child_pids.sort_unstable();
    }

    fn visit(
        pid: u32,
        children: &HashMap<u32, Vec<u32>>,
        visited: &mut HashSet<u32>,
        by_pid: &HashMap<u32, &ProcessItem>,
        order: &mut Vec<ProcessLiveKey>,
    ) {
        if !visited.insert(pid) {
            return;
        }
        if let Some(child_pids) = children.get(&pid) {
            for child_pid in child_pids {
                visit(*child_pid, children, visited, by_pid, order);
            }
        }
        if let Some(identity) = by_pid
            .get(&pid)
            .and_then(|process| ProcessLiveKey::from_process(process))
        {
            order.push(identity);
        }
    }

    let mut order = Vec::new();
    visit(
        root_process.pid,
        &children,
        &mut HashSet::new(),
        &by_pid,
        &mut order,
    );
    order
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessBatchTargetResult {
    Applied,
    IdentityUnavailable,
    IdentityChanged,
    Failed(#[serde(with = "process_batch_failure_wire")] FailureKind),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessBatchResult {
    pub intent: ProcessBatchIntent,
    pub targets: Vec<(FrozenProcessIdentity, ProcessBatchTargetResult)>,
}

impl ProcessBatchResult {
    #[must_use]
    pub fn applied_count(&self) -> usize {
        self.targets
            .iter()
            .filter(|(_, result)| matches!(result, ProcessBatchTargetResult::Applied))
            .count()
    }
}

pub fn execute_process_batch_with(
    intent: ProcessBatchIntent,
    live: &[ProcessItem],
    mut execute: impl FnMut(ProcessBatchAction, &FrozenProcessIdentity) -> Result<(), FailureKind>,
) -> ProcessBatchResult {
    let targets = intent
        .targets
        .iter()
        .cloned()
        .map(|identity| {
            let result = if identity.authoritative_start_token().is_none() {
                ProcessBatchTargetResult::IdentityUnavailable
            } else if live.iter().any(|process| {
                process.pid == identity.pid
                    && process.name == identity.name
                    && process.current_start_token() == identity.authoritative_start_token()
            }) {
                execute(intent.action, &identity)
                    .map(|()| ProcessBatchTargetResult::Applied)
                    .unwrap_or_else(ProcessBatchTargetResult::Failed)
            } else {
                ProcessBatchTargetResult::IdentityChanged
            };
            (identity, result)
        })
        .collect();
    ProcessBatchResult { intent, targets }
}

/// Preserve the schema-v1 per-target failure tokens while the in-memory
/// contract uses the complete shared [`FailureKind`] vocabulary.
///
/// The three legacy spellings remain intentional:
/// `not_found_or_reused`, `provider_unavailable`, and `other`.
pub(crate) const fn process_batch_failure_wire_code(failure: FailureKind) -> &'static str {
    match failure {
        FailureKind::Unsupported => "unsupported",
        // RequiresEscalation is an escalatable denial (the Intel PMU path); the
        // legacy schema has no escalation token, and this wire path never
        // carries a process-batch escalation, so fold it into the denial token.
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => "permission_denied",
        FailureKind::MissingDependency => "missing_dependency",
        FailureKind::TimedOut => "timed_out",
        FailureKind::IdentityChanged => "not_found_or_reused",
        FailureKind::TemporarilyUnavailable => "provider_unavailable",
        FailureKind::Rejected => "rejected",
        FailureKind::ProviderFault => "other",
    }
}

fn parse_process_batch_failure_wire_code(code: &str) -> Option<FailureKind> {
    match code {
        "unsupported" => Some(FailureKind::Unsupported),
        "permission_denied" => Some(FailureKind::PermissionDenied),
        "missing_dependency" => Some(FailureKind::MissingDependency),
        "timed_out" => Some(FailureKind::TimedOut),
        "not_found_or_reused" | "identity_changed" => Some(FailureKind::IdentityChanged),
        "provider_unavailable" | "temporarily_unavailable" => {
            Some(FailureKind::TemporarilyUnavailable)
        }
        "rejected" => Some(FailureKind::Rejected),
        "other" | "provider_fault" => Some(FailureKind::ProviderFault),
        _ => None,
    }
}

mod process_batch_failure_wire {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    use super::{
        FailureKind, parse_process_batch_failure_wire_code, process_batch_failure_wire_code,
    };

    pub fn serialize<S>(failure: &FailureKind, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(process_batch_failure_wire_code(*failure))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<FailureKind, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code = String::deserialize(deserializer)?;
        parse_process_batch_failure_wire_code(&code)
            .ok_or_else(|| D::Error::custom(format!("unknown process batch failure code: {code}")))
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_control_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_control_batch_wire_tests.rs"]
mod batch_wire_tests;
