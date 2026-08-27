//! Bevy UI fourth frontend — M0 skeleton, bootstrap spread
//! ([docs/BEVY_UI_FRONTEND.md](../../docs/BEVY_UI_FRONTEND.md)).
//!
//! Peer surface to the GPUI/Iced/TUI frontends: this crate renders the same
//! neutral shell projections with Bevy 0.19's official two-piece UI base —
//! `bevy_ui` + `bevy_ui_widgets` — composing static structure declaratively
//! with `bsn!` and binding dynamic state through observers and required
//! components. It owns these seams:
//!
//! - [`runtime`]: the process-wide platform client, acquired once through the
//!   app-host `OnceLock` cache pattern (charter boundary 5). A window rebuild
//!   reuses the cached handle; only process start spawns the runtime.
//! - [`drain`]: the per-frame `PreUpdate` event-port drain that folds platform
//!   batches into the shell and commits refresh intents (boundary 4). The
//!   seam core is window-free and headlessly testable.
//! - [`palette`]: the toolkit-neutral theme tokens mapped onto bevy colors and
//!   type metrics (boundary 2 — the adapter lives in this crate, never behind
//!   a `theme` feature).
//! - [`app`]: the frontend-owned route model (eight pages), keyboard routing
//!   through the shared command router, and the [`app::ShellTrack`] seam
//!   pages read the folded projection through.
//! - [`pages`]: the eight page modules — placeholder bodies until their
//!   milestones; each page agent fills exactly one file.
//! - [`widgets`]: the owned component layer (table/sparkline cores + bsn!
//!   render adapters, menu/dialog skeletons).
//! - [`window`]: the bsn! app shell (header + nav rail + content slot) and
//!   the observers that keep it live.
//!
//! Bevy types never cross this crate's public API: [`run_placeholder_window`]
//! answers with a plain [`std::process::ExitCode`], and the window module
//! stays crate-private.

#![forbid(unsafe_code)]
// This crate's docs deliberately navigate to its internal seams — the
// [`app`] route model, the drain events, the page observers — while the
// public API stays minimal (bevy types and internals never cross it).
// Every module-level doc that bracket-links one of those private items
// would otherwise trip rustdoc's private-link lint under `-D warnings`.
#![allow(rustdoc::private_intra_doc_links)]

pub mod app;
pub mod drain;
pub mod input_contract;
pub mod pages;
pub mod palette;
pub mod runtime;
pub mod widgets;
mod window;

use std::process::ExitCode;

/// Open the app-shell window and run it to completion.
///
/// The composition order matches the other frontends' launchers: the shared
/// platform runtime is resolved first (typed failure → non-zero exit before
/// any window exists), then the bevy `App` is built around the cached handle.
pub fn run_placeholder_window() -> ExitCode {
    match runtime::shared_platform_runtime() {
        Ok(shared) => window::run(shared),
        Err(failure) => {
            eprintln!("taskforest-b: {failure}");
            ExitCode::FAILURE
        }
    }
}

/// Open the real Bevy window with the explicit deterministic demo fixture.
/// This is the entry used by the Wayland capture matrix; it never starts a
/// native collector or reads host telemetry.
pub fn run_demo_window() -> ExitCode {
    window::run_demo(runtime::demo_platform_runtime())
}
