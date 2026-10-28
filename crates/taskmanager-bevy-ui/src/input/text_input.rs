//! Text input editing state for Bevy UI.
//!
//! The transitions live in the shared [`TextEditState`] contract (the same
//! UTF-8 caret/selection machine the TUI and the read-only selection surfaces
//! use); this resource only gives that state a Bevy home.

use bevy::prelude::Resource;
use taskmanager_ui_contract::TextEditState;

/// State for text input editing (cursor position and selection anchor).
///
/// The shape owns no clipboard buffer: no system-clipboard read or write is
/// wired, so the editor offers character editing only and binds no clipboard
/// chords.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextInputState {
    pub(crate) edit: TextEditState,
}
