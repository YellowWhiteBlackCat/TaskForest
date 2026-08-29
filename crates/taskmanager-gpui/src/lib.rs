//! GPUI desktop frontend for TaskForest.
//!
//! The crate owns the GPUI product surface, including views, the theme
//! adapter, and the GPUI event loop. Native composition is supplied by the
//! toolkit-neutral `taskmanager-app-host`; the root `taskmanager` package only
//! selects this frontend through its `ui-gpui` feature and dispatches the
//! shared CLI entry point.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

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
