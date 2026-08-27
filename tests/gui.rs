#![cfg(feature = "ui-gpui")]
#![allow(linker_messages)]

#[path = "gui/gpui_behavior.rs"]
mod gpui_behavior;

#[path = "gui/keyboard_behavior.rs"]
mod keyboard_behavior;

#[path = "gui/accessibility_behavior.rs"]
mod accessibility_behavior;

#[path = "gui/dashboard_contract.rs"]
mod dashboard_contract;

#[path = "gui/multi_window_behavior.rs"]
mod multi_window_behavior;

#[path = "gui/mission_center_acceptance.rs"]
mod mission_center_acceptance;

#[path = "gui/dual_track_policy_parity.rs"]
mod dual_track_policy_parity;
