//! The shared Startup-page search predicate.
//!
//! Frontends own their input widgets, but every renderer's visible-row
//! projection must use this one rule for trimming, name/command matching and
//! case folding. Status filtering and sorting remain the caller's typed rules.

use taskforest_core::core::text::contains_ascii_ci;

/// Whether one startup row's text survives the Startup-page search query.
///
/// The query is trimmed before matching, and an empty query keeps every row.
/// Matching is ASCII case-insensitive over the name or the command.
#[must_use]
pub fn matches_startup_text(name: &str, exec: &str, query: &str) -> bool {
    let query = query.trim();
    query.is_empty() || contains_ascii_ci(name, query) || contains_ascii_ci(exec, query)
}

#[cfg(test)]
#[path = "../tests/headless/startup_query.rs"]
mod tests;
