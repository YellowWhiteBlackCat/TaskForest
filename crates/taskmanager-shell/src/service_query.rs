//! The shared Services-page search predicate.
//!
//! Every frontend keeps the Services query in the shell
//! ([`crate::ShellApp::services_query`]); this module is the single rule that
//! decides which rows that query keeps. GPUI, Iced, Bevy, and the TUI all
//! consume it so one typed name can never filter differently per renderer.

use taskforest_core::core::services::ServiceItem;
use taskforest_core::core::text::contains_ascii_ci;

/// Whether one service's text survives the Services-page search query.
///
/// The query is trimmed before matching, and an empty query keeps every row.
/// Matching is ASCII case-insensitive over the name or the description, the
/// same convention the process filter uses. Renderers whose row model is not
/// a [`ServiceItem`] (Iced's owned `ServiceRow`) call this directly.
#[must_use]
pub fn matches_service_text(name: &str, description: &str, query: &str) -> bool {
    let query = query.trim();
    query.is_empty() || contains_ascii_ci(name, query) || contains_ascii_ci(description, query)
}

/// Whether one service survives the Services-page search query.
#[must_use]
pub fn matches_service_query(service: &ServiceItem, query: &str) -> bool {
    matches_service_text(&service.name, &service.description, query)
}

#[cfg(test)]
#[path = "../tests/headless/service_query.rs"]
mod tests;
