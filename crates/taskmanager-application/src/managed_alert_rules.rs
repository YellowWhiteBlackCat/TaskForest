//! Canonical managed-alert-rule state and edit reducer.

use taskmanager_core::alerts::{
    AlertRule, AlertRuleConflictPolicy, AlertRuleTransferEntry, AlertRuleTransferError,
    merge_alert_rule_entries,
};

/// One durable alert rule and whether it participates in evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct ManagedAlertRule {
    pub rule: AlertRule,
    pub enabled: bool,
}

impl ManagedAlertRule {
    #[must_use]
    pub const fn new(rule: AlertRule, enabled: bool) -> Self {
        Self { rule, enabled }
    }
}

impl From<AlertRuleTransferEntry> for ManagedAlertRule {
    fn from(entry: AlertRuleTransferEntry) -> Self {
        Self::new(entry.rule, entry.enabled)
    }
}

impl From<&ManagedAlertRule> for AlertRuleTransferEntry {
    fn from(managed: &ManagedAlertRule) -> Self {
        Self::new(managed.rule.clone(), managed.enabled)
    }
}

/// Whether an imported document augments or replaces the canonical list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertRuleImportMode {
    Merge(AlertRuleConflictPolicy),
    Replace,
}

/// Complete semantic edit vocabulary for the managed rule authority.
#[derive(Clone, Debug, PartialEq)]
pub enum ManagedAlertRuleEdit {
    Toggle {
        rule_id: String,
    },
    Add(ManagedAlertRule),
    Update {
        target_id: String,
        managed: ManagedAlertRule,
    },
    Remove {
        rule_id: String,
    },
    Import {
        rules: Vec<ManagedAlertRule>,
        mode: AlertRuleImportMode,
    },
}

/// Observable reducer disposition. A missing stable target is a safe no-op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedAlertRuleEditOutcome {
    Applied,
    Unchanged,
    MissingTarget,
}

impl ManagedAlertRuleEditOutcome {
    #[must_use]
    pub const fn changed(self) -> bool {
        matches!(self, Self::Applied)
    }
}

pub(crate) fn reduce_managed_alert_rules(
    current: &mut Vec<ManagedAlertRule>,
    edit: ManagedAlertRuleEdit,
) -> Result<ManagedAlertRuleEditOutcome, AlertRuleTransferError> {
    match edit {
        ManagedAlertRuleEdit::Toggle { rule_id } => {
            let Some(managed) = current
                .iter_mut()
                .find(|managed| managed.rule.id == rule_id)
            else {
                return Ok(ManagedAlertRuleEditOutcome::MissingTarget);
            };
            managed.enabled = !managed.enabled;
            Ok(ManagedAlertRuleEditOutcome::Applied)
        }
        ManagedAlertRuleEdit::Add(managed) => {
            let existing = transfer_entries(current);
            let imported = [AlertRuleTransferEntry::from(&managed)];
            let merged =
                merge_alert_rule_entries(&existing, &imported, AlertRuleConflictPolicy::Reject)?;
            *current = managed_rules(merged.entries);
            Ok(ManagedAlertRuleEditOutcome::Applied)
        }
        ManagedAlertRuleEdit::Update { target_id, managed } => {
            let Some(index) = current
                .iter()
                .position(|existing| existing.rule.id == target_id)
            else {
                return Ok(ManagedAlertRuleEditOutcome::MissingTarget);
            };
            if current[index] == managed {
                return Ok(ManagedAlertRuleEditOutcome::Unchanged);
            }
            let mut candidate = current.clone();
            candidate[index] = managed;
            validate(&candidate)?;
            *current = candidate;
            Ok(ManagedAlertRuleEditOutcome::Applied)
        }
        ManagedAlertRuleEdit::Remove { rule_id } => {
            let Some(index) = current
                .iter()
                .position(|managed| managed.rule.id == rule_id)
            else {
                return Ok(ManagedAlertRuleEditOutcome::MissingTarget);
            };
            current.remove(index);
            Ok(ManagedAlertRuleEditOutcome::Applied)
        }
        ManagedAlertRuleEdit::Import { rules, mode } => {
            let imported = transfer_entries(&rules);
            let next = match mode {
                AlertRuleImportMode::Merge(policy) => {
                    merge_alert_rule_entries(&transfer_entries(current), &imported, policy)?.entries
                }
                AlertRuleImportMode::Replace => {
                    merge_alert_rule_entries(&[], &imported, AlertRuleConflictPolicy::Reject)?
                        .entries
                }
            };
            let next = managed_rules(next);
            if *current == next {
                return Ok(ManagedAlertRuleEditOutcome::Unchanged);
            }
            *current = next;
            Ok(ManagedAlertRuleEditOutcome::Applied)
        }
    }
}

fn validate(rules: &[ManagedAlertRule]) -> Result<(), AlertRuleTransferError> {
    merge_alert_rule_entries(
        &[],
        &transfer_entries(rules),
        AlertRuleConflictPolicy::Reject,
    )
    .map(|_| ())
}

fn transfer_entries(rules: &[ManagedAlertRule]) -> Vec<AlertRuleTransferEntry> {
    rules.iter().map(AlertRuleTransferEntry::from).collect()
}

fn managed_rules(entries: Vec<AlertRuleTransferEntry>) -> Vec<ManagedAlertRule> {
    entries.into_iter().map(ManagedAlertRule::from).collect()
}
