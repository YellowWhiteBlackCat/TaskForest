//! Bevy UI fourth frontend — peer surface to the GPUI/Iced/TUI frontends
//! ([docs/BEVY_UI_FRONTEND.md](../../docs/BEVY_UI_FRONTEND.md)).
//!
//! This crate renders the same neutral shell projections with Bevy 0.19's
//! official two-piece UI base — `bevy_ui` + `bevy_ui_widgets` — composing
//! static structure declaratively with `bsn!` and binding dynamic state
//! through observers and required components. It owns these seams:
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
//! - [`app`]: the frontend-owned route model (nine pages), keyboard routing
//!   through the shared command router, and the [`app::ShellTrack`] seam
//!   pages read the folded projection through.
//! - [`input`]: the real-input seam — Bevy keyboard events forwarded
//!   through the shell's own routers, with the effect bridge to the drain.
//! - [`confirmation`]: the shell's armed destructive-action gate rendered as
//!   one modal surface with typed confirm/dismiss paths.
//! - [`icons`]: the semantic icon bridge — `IconId` → shared RGBA bitmaps →
//!   tinted `ImageNode`s, with the no-decoration-glyph tofu law.
//! - [`semantic`]: the accessibility seam — the shared `SemanticSnapshot`
//!   vocabulary mapped onto bevy's AccessKit nodes.
//! - [`pages`]: the nine page modules — page adapters over the shared shell
//!   projection; each page owns its scene and refresh seams.
//! - [`widgets`]: the owned component layer (table/chart cores + bsn! render
//!   adapters and the keyboard-first action menu).
//! - [`window`]: the bsn! app shell (nav strip + status band + content slot)
//!   and the observers that keep it live.
//!
//! Bevy types never cross this crate's public API: [`run_window`]
//! answers with a plain [`std::process::ExitCode`], and the window module
//! stays crate-private.

#![forbid(unsafe_code)]
// This crate's docs deliberately navigate to its internal seams — the
// [`app`] route model, the drain events, the page observers — while the
// public API stays minimal (bevy types and internals never cross it).
// Every module-level doc that bracket-links one of those private items
// would otherwise trip rustdoc's private-link lint under `-D warnings`.
#![allow(rustdoc::private_intra_doc_links)]

// A release artifact is a platform variant, never a hardware-vendor variant
// (ADR-006/051). Developers may use reduced debug builds to exercise fallback
// paths, but a distributable binary must carry every backend in the standard
// hardware set.
#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!(
    "release builds require the default `hardware-all` feature; \
     vendor-specific TaskForest artifacts are not supported"
);

pub mod app;
pub mod bindings;
pub mod capabilities;
pub mod confirmation;
pub mod demo_fixture;
pub mod drain;
pub mod functional;
pub mod icons;
pub mod input;
pub mod input_contract;
pub mod menu_modal;
pub mod pages;
pub mod palette;
pub mod runtime;
pub mod semantic;
pub(crate) mod tray;
pub mod widgets;
mod window;

#[cfg(test)]
#[path = "../tests/headless/visual_parity.rs"]
mod visual_parity_tests;

use std::process::ExitCode;

/// Open the app-shell window and run it to completion.
///
/// The composition order matches the other frontends' launchers: the shared
/// platform runtime is resolved first (typed failure → non-zero exit before
/// any window exists), then the bevy `App` is built around the cached handle.
pub fn run_window() -> ExitCode {
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
