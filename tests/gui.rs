#![cfg(feature = "test-support")]
// `linker_messages` is emitted by rustc when the platform linker prints a
// warning (an MSVC concern); scope the allow to that environment instead of
// silencing it everywhere.
#![cfg_attr(target_env = "msvc", allow(linker_messages))]

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
