//! The TaskForest binary library (ADR-029): one binary, three UI shapes.
//!
//! Exactly one `ui-*` feature is enabled per build (enforced by build.rs):
//! `ui-gpui` (default) compiles the GPUI desktop frontend, `ui-tui` the
//! ratatui frontend, `ui-iced` the iced frontend. The UI-neutral CLI
//! (`cli`, `--json`, `--suggest-thresholds`, `--gpu-engines`) and the
//! re-exported neutral core are compiled in every shape.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

// A release artifact is a platform variant, never a hardware-vendor variant.
// Developers may use reduced debug builds to exercise fallback paths, but a
// distributable binary must carry every backend in the standard hardware set.
#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!(
    "release builds require the default `hardware-all` feature; \
     vendor-specific TaskManager artifacts are not supported"
);

pub mod cli;
pub mod cli_gpu_engines;
pub mod cli_process_gpu;
pub mod frontend;
// i18n lives in the shared `taskmanager-application` crate so every frontend
// (gpui/tui/iced) can consume it; re-exported here so existing `crate::i18n`
// call sites keep resolving unchanged.
pub use taskmanager_application::i18n;

pub use taskmanager_core::core;
pub use taskmanager_core::core::*;
