//! Renderer-local focus, modifier, context-menu and motion ownership.

use super::{ContextMenuState, FocusTarget, ModalAppear, WarmupSpin};

#[derive(Default)]
pub(crate) struct InputState {
    pub(crate) focused_control: Option<FocusTarget>,
    pub(crate) modal_restore: Option<FocusTarget>,
    pub(crate) modifiers: iced::keyboard::Modifiers,
    pub(crate) context_menu: ContextMenuState,
    pub(crate) modal_appear: Option<ModalAppear>,
    pub(crate) warmup_spin: Option<WarmupSpin>,
}
