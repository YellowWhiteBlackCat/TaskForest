//! Shared process-state buckets for frontend-owned segmented filters.
//!
//! The shell owns the filtered row projection, while each renderer owns the
//! control that selects a bucket. Keeping the classifier here means selection,
//! keyboard navigation, actions, and rendered rows all consume the same list.

use taskmanager_application::i18n::t;
use taskmanager_core::core::process::ProcessItem;

/// The six process-state buckets exposed by the Applications filter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessStatusFilter {
    /// Keep every process regardless of its state.
    #[default]
    All,
    Running,
    Sleeping,
    Stopped,
    Zombie,
    Other,
}

impl ProcessStatusFilter {
    /// The complete segmented-control order used by the frontends.
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Running,
        Self::Sleeping,
        Self::Stopped,
        Self::Zombie,
        Self::Other,
    ];

    /// Localized control label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => t("common.all"),
            Self::Running => t("proc.status_running"),
            Self::Sleeping => t("proc.status_sleeping"),
            Self::Stopped => t("proc.status_stopped"),
            Self::Zombie => t("proc.status_zombie"),
            Self::Other => t("proc.status_other"),
        }
    }

    /// Stable non-localized control identity.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Running => "running",
            Self::Sleeping => "sleeping",
            Self::Stopped => "stopped",
            Self::Zombie => "zombie",
            Self::Other => "other",
        }
    }

    /// Classify full provider words first, then tolerate Linux `/proc` state
    /// letters. Full-word precedence is required because both Sleeping and
    /// Stopped begin with `s`.
    #[must_use]
    pub fn classify(status: &str) -> Self {
        let normalized = status.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "running" => Self::Running,
            "sleeping" => Self::Sleeping,
            "stopped" => Self::Stopped,
            "zombie" => Self::Zombie,
            _ => match normalized.chars().next() {
                Some('r') => Self::Running,
                Some('s') => Self::Sleeping,
                Some('t') => Self::Stopped,
                Some('z') => Self::Zombie,
                _ => Self::Other,
            },
        }
    }

    /// Whether a process state belongs to this filter.
    #[must_use]
    pub fn matches(self, status: &str) -> bool {
        self == Self::All || self == Self::classify(status)
    }
}

/// Match a process against a query string, supporting structured prefix selectors
/// (`pid:`, `user:`, `status:`, `cmd:`, `name:`) or general multi-field search.
#[must_use]
pub fn matches_process_query(process: &ProcessItem, query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    let user = process.current_user().unwrap_or_default();
    let tokens: Vec<&str> = q.split_whitespace().collect();
    let mut has_prefix = false;
    for token in &tokens {
        if let Some((prefix, val)) = token.split_once(':') {
            has_prefix = true;
            let val = val.trim();
            let matched = match prefix.to_ascii_lowercase().as_str() {
                "pid" => process.pid.to_string().contains(val),
                "user" => taskmanager_core::core::text::contains_ascii_ci(&user, val),
                "status" => taskmanager_core::core::text::contains_ascii_ci(&process.status, val),
                "cmd" | "cmdline" => {
                    taskmanager_core::core::text::contains_ascii_ci(&process.cmdline, val)
                }
                "name" => taskmanager_core::core::text::contains_ascii_ci(&process.name, val),
                _ => {
                    taskmanager_core::core::text::contains_ascii_ci(&process.name, token)
                        || process.pid.to_string().contains(token)
                        || taskmanager_core::core::text::contains_ascii_ci(&user, token)
                        || taskmanager_core::core::text::contains_ascii_ci(&process.cmdline, token)
                }
            };
            if !matched {
                return false;
            }
        }
    }
    if has_prefix {
        return true;
    }
    let digit_query = q.bytes().all(|b| b.is_ascii_digit());
    taskmanager_core::core::text::contains_ascii_ci(&process.name, q)
        || (digit_query && process.pid.to_string().contains(q))
        || taskmanager_core::core::text::contains_ascii_ci(&user, q)
        || taskmanager_core::core::text::contains_ascii_ci(&process.cmdline, q)
}

#[cfg(test)]
#[path = "../tests/headless/shell_process_filter.rs"]
mod tests;
