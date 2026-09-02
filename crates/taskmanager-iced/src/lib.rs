//! Iced frontend product for TaskForest (ADR-027/028/051).
//!
//! This product is a peer to the GPUI desktop shell, Ratatui TUI and Bevy
//! product: the renderer-independent state machine lives in
//! `taskmanager-shell`, design tokens come from the neutral
//! `taskmanager-theme`, and this crate maps them onto Iced 0.14.
//!
//! Modules:
//!
//! - [`theme`] — the neutral skin registry mapped onto iced colors.
//! - [`keys`] — iced keyboard events normalized into the shared shell
//!   key vocabulary.
//! - `input_modality` — the renderer-local input-modality tracker (the
//!   focus-visible source) and the root pointer-press observer.
//! - [`focus`] — the real Iced `operation::Focusable` adapter for modal
//!   controls.
//! - [`a11y`] — a bounded semantic snapshot projection; no native bridge is
//!   claimed until an Iced accessibility adapter is linked and evidenced.
//! - [`app`] — the iced [`IcedApp`] state: platform data flow, refresh
//!   scheduling, and the `Message` loop.
//! - `perf_history` — the frontend-local bounded per-process ring for the
//!   details overlay's Performance tab; the system-wide headline chart reads
//!   the shared shell `LiveGraphHistory` series (G-02, ADR-028).
//! - `perf_chart` — a minimal iced `Canvas` line/area chart for the
//!   Performance page (iced ships no chart widget).
//! - `app_history_chart` — a one-series `Canvas` sparkline for the
//!   App-history page rows (reuses the Performance chart's point projection).
//! - [`ui`] — the iced view layer (pages, gauges, process table).

#![forbid(unsafe_code)]

// A release artifact is a platform variant, never a hardware-vendor variant
// (ADR-006/051). Developers may use reduced debug builds to exercise fallback
// paths, but a distributable binary must carry every backend in the standard
// hardware set.
#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!(
    "release builds require the default `hardware-all` feature; \
     vendor-specific TaskForest artifacts are not supported"
);

pub mod a11y;
pub mod app;
pub(crate) mod app_history_chart;
pub mod capabilities;
pub(crate) mod capture;
pub mod export;
pub mod focus;
pub(crate) mod font_catalog;
pub mod functional;
pub mod i18n;
pub(crate) mod icons;
pub(crate) mod input_modality;
pub mod keys;
pub(crate) mod perf_chart;
pub(crate) mod perf_history;
pub mod run;
pub mod saved_views;
pub(crate) mod text_metrics;
pub mod theme;
pub(crate) mod tray;
pub(crate) mod trend_strip;
pub mod ui;

pub mod theme_binding;

pub use app::{IcedApp, Message};

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
mod test_support;
pub use run::run;
