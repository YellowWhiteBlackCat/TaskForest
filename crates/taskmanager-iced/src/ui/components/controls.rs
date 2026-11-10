//! The Iced ports of the GPUI primitives/inputs activation and layout layer.
//!
//! Every interactive control paints the ONE shared Card glass through
//! [`crate::theme::control_glass`] and resolves its geometry through the
//! shared `ControlRole` matrix. States only reweight paint; focus rings read
//! the shared keyboard-visible ring token. Native Iced widgets keep pointer,
//! caret, IME and selection paths, while [`crate::focus::FocusableButton`]
//! owns Tab stops and Enter/Space activation. Copy is resolved through
//! `taskmanager-application::i18n` by the caller.

mod activation;
mod layout;
mod material;
#[path = "controls/spinner.rs"]
mod spinner_module;
pub(crate) mod state;
mod strip;
mod text;

pub(crate) use activation::{check_box, glass_button, icon_button, pill};
pub(crate) use layout::{label, section_header, toolbar};
pub(crate) use material::{
    focusable_control_button_intrinsic, focusable_control_button_owned,
    focusable_icon_label_button_intrinsic,
};
pub(crate) use spinner_module::spinner;
pub(crate) use state::ButtonTone;
pub(crate) use strip::bounded_toolbar;
pub(crate) use text::plain_text_input;

#[cfg(test)]
#[path = "../../../tests/gui/ui/components_controls_tests.rs"]
mod components_controls_tests;
