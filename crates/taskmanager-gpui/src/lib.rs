//! GPUI desktop frontend product (`taskforest-g`) for TaskForest.
//!
//! The crate owns the GPUI product surface, the GPUI event loop, and this
//! product's binary. Native composition is supplied by the toolkit-neutral
//! `taskmanager-app-host`; the shared CLI harness (`taskmanager-cli`,
//! ADR-051) owns argv parsing, the neutral modes, and the capability seam.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

// A release artifact is a platform variant, never a hardware-vendor variant
// (ADR-006). Developers may use reduced debug builds to exercise fallback
// paths, but a distributable binary must carry every backend in the standard
// hardware set.
#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!(
    "release builds require the default `hardware-all` feature; \
     vendor-specific TaskForest artifacts are not supported"
);

mod assets;
pub mod gpui_app;
mod run;
mod window_presentation;

// Windows-only evidence mode (`--capture-window`): real-window self-capture
// via Windows.Graphics.Capture. The module is empty elsewhere so the frontend
// still builds on Linux/macOS.
#[cfg(target_os = "windows")]
pub mod capture;

pub use gpui_app::{RootView, init};
pub use run::run;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
mod test_support;
