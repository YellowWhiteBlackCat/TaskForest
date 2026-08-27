//! Semantic icon registry plus toolkit-specific rendering adapters (ADR-017).
//!
//! The toolkit-neutral semantic identity ([`taskmanager_ui_contract::IconId`])
//! stays in `taskmanager-ui-contract`; the embedded SVG assets stay in
//! `taskmanager-assets`. This crate owns the GPUI mapping between them:
//!
//! - [`path` function] — resolve an [`IconId`](taskmanager_ui_contract::IconId) to the
//!   embedded asset path used by GPUI.
//! - [`path`] — resolve an [`IconId`](taskmanager_ui_contract::IconId) to the
//!   embedded SVG asset path.
//! - [`asset_bytes`] — retrieve the same embedded SVG bytes for another
//!   frontend's native vector widget.
//! - [`icon`] — build a GPUI SVG icon element. Color inherits from the
//!   surrounding text style at layout time, and callers can keep chaining
//!   GPUI style methods (`.size(..)`, `.text_color(..)`, …) on the result.
//!
//! No `gpui_component` types are used or re-exported.

#![forbid(unsafe_code)]
#![deny(clippy::wildcard_imports)]

#[cfg(feature = "gpui")]
mod application;
mod path;
#[cfg(feature = "gpui")]
mod svg;

#[cfg(feature = "gpui")]
pub use application::{ApplicationImageFormat, application_image};
pub use path::{asset_bytes, path};
#[cfg(feature = "gpui")]
pub use svg::icon;
