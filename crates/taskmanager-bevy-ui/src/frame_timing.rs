//! Bevy frame-timing sink — product env binding only.
//!
//! The Bevy-scoped plugin, systems, recorder and wire record are owned by
//! `zhuge-ui-kit-bevy::frame_timing`. This module names TaskForest's opt-in
//! variables and capture-evidence style; a normal launch installs nothing.

use zhuge_ui_core::frame_timing::record::FrameTimingIdentity;
use zhuge_ui_kit_bevy::frame_timing::{BevyFrameTimingEnv, FrameReceiptStyle};

/// The product's frame-timing environment.
///
/// The composition edge calls the kit owner directly with this value — the same
/// shape `window.rs` already uses for `install_sync_frame_capture`. A one-line
/// product-local `install(app) -> bool` wrapper was a second address for the
/// owner's install, and two products had spelled it identically.
#[must_use]
pub(crate) fn env() -> BevyFrameTimingEnv {
    let soak_receipt = std::env::var_os("TM_BEVY_FRAME_TIMING_RECEIPT").is_some();
    BevyFrameTimingEnv {
        sink_var: "TM_FRAME_TIMING_SINK",
        budget_var: "TM_FRAME_TIMING_BUDGET_MS",
        default_budget_ms: 16.0,
        receipt_vars: if soak_receipt {
            &["TM_BEVY_FRAME_TIMING_RECEIPT"]
        } else {
            &["TM_BEVY_SYNC_FRAME_DIR"]
        },
        receipt_style: if soak_receipt {
            FrameReceiptStyle::RunLevel
        } else {
            FrameReceiptStyle::PerFramePng
        },
        identity: FrameTimingIdentity::new("taskforest", "taskforest-bevy", "bevy"),
        max_samples: 240,
    }
}

#[cfg(test)]
#[path = "../tests/headless/frame_timing.rs"]
mod tests;
