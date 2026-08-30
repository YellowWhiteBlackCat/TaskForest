//! Host-neutral invariants over shared process rows.
//!
//! The checks are deliberately lenient (kernel threads may report zero
//! memory, parents may exit between snapshots) so every adapter can satisfy
//! them on any host while still catching fabricated or corrupt rows.

use std::collections::HashSet;

use taskmanager_core::process::ProcessItem;

/// Every row must be internally consistent and the snapshot must not contain
/// duplicate identities.
pub fn assert_process_rows_consistent(rows: &[ProcessItem]) -> Result<(), String> {
    let mut violations = Vec::new();
    let mut seen = HashSet::with_capacity(rows.len());
    for row in rows {
        if !seen.insert(row.pid) {
            violations.push(format!("duplicate pid {} in one snapshot", row.pid));
        }
        if row.name.trim().is_empty() {
            violations.push(format!("pid {} has an empty name", row.pid));
        }
        if let Some(cpu_usage) = row.current_cpu_percentage() {
            if !cpu_usage.is_finite() {
                violations.push(format!("pid {} reported non-finite CPU usage", row.pid));
            }
            if !(0.0..=100.0).contains(&cpu_usage) {
                violations.push(format!(
                    "pid {} reported CPU {cpu_usage:.3} outside [0,100]",
                    row.pid
                ));
            }
        }
        // The history projection renders a missing channel as a NaN typed gap
        // (charts break the line instead of plotting a fabricated zero), so
        // only an infinite value — corrupt arithmetic, never an honest
        // measurement — is a violation here. The current-sample check above
        // still pins measured magnitudes.
        if row.cpu_history.iter().any(|value| value.is_infinite()) {
            violations.push(format!(
                "pid {} reported an infinite CPU history value",
                row.pid
            ));
        }
        if row.parent_pid == Some(row.pid) {
            violations.push(format!("pid {} is its own parent", row.pid));
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("; "))
    }
}

#[cfg(test)]
#[path = "../tests/headless/process_contract.rs"]
mod tests;
