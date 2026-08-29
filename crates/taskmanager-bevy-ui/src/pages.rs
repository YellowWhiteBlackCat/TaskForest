//! The nine page modules of the Bevy frontend.
//!
//! **Page-agent contract.** Each page owns exactly one file under `pages/`
//! and exposes one function:
//!
//! ```ignore
//! pub(crate) fn content(context: &PageContext<'_>) -> impl Scene
//! ```
//!
//! - `context.shell` (`&ShellApp`) is the data entry: read the projection
//!   with `context.shell.projection()` (processes, services, startup,
//!   sessions, telemetry, alerts, source statuses, revisions) and use the
//!   memoized row projections (`visible_processes`, `sorted_services`, …)
//!   for table rows. Read-only, always.
//! - `context.palette` (`&UiPalette`) is the only color/type source — never
//!   a literal, never Feathers.
//! - `context.body` / `context.heading` are ready type metrics; the font
//!   handle is stamped by the text-role observers in [`crate::window`], so
//!   pages spawn `Text` + a role marker and never touch font assets.
//! - `context.history` is the immutable application-history projection from
//!   the app-host connector; it is not a second live-process join authority.
//! - Dynamic refresh: pages register their own observers on
//!   `crate::drain::ShellProjectionFolded` (fired whenever the drain folded
//!   platform batches this frame) and re-read the shell through the
//!   [`crate::app::ShellTrack`] system param.
//! - Shared widgets live in [`crate::widgets`] (table, sparkline, menu,
//!   dialog); shared files (app.rs, window.rs, palette.rs, drain.rs) are
//!   NOT touched by page work — this reservation is the whole point of the
//!   one-file-per-page shape.

pub mod alerts;
pub mod history;
pub mod performance;
pub mod process_tree;
pub mod processes;
pub mod services;
pub mod sessions;
pub mod settings;
pub mod startup;
pub mod system;

#[cfg(test)]
#[path = "../tests/headless/pages.rs"]
mod tests;
