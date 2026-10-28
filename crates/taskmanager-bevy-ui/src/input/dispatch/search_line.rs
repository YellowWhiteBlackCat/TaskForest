//! The shell-backed search-line editor (the fallback used while the native
//! text input is not mounted or focused).

use bevy::input::keyboard::KeyCode;
use taskmanager_shell::{SearchSlot, ShellApp};
use taskmanager_ui_contract::TextMotion;

use super::KeyPress;
use crate::input::{TextInputState, commit_slot_query};

/// Applies one press to the search line, committing every query edit to the
/// slot that owns it (the shell is the single owner of both search texts).
/// `true` when the line editor consumed the press; an unconsumed press keeps
/// falling through the chain.
pub(super) fn edit_search_line(
    press: KeyPress,
    state: &mut TextInputState,
    query: &mut String,
    shell: &mut ShellApp,
    slot: SearchSlot,
) -> bool {
    let is_ctrl = press.modifiers.control;
    match press.key_code {
        KeyCode::ArrowLeft => {
            let motion = if is_ctrl {
                TextMotion::WordLeft
            } else {
                TextMotion::Left
            };
            state.edit.move_caret(query, motion, false);
        }
        KeyCode::ArrowRight => {
            let motion = if is_ctrl {
                TextMotion::WordRight
            } else {
                TextMotion::Right
            };
            state.edit.move_caret(query, motion, false);
        }
        KeyCode::Home => {
            state.edit.move_caret(query, TextMotion::TextStart, false);
        }
        KeyCode::End => {
            state.edit.move_caret(query, TextMotion::TextEnd, false);
        }
        KeyCode::Backspace => {
            let changed = if is_ctrl {
                state.edit.delete_word_backward(query)
            } else {
                state.edit.delete_backward(query)
            };
            if changed {
                commit_slot_query(shell, slot, query);
            }
        }
        KeyCode::Delete => {
            if state.edit.delete_forward(query) {
                commit_slot_query(shell, slot, query);
            }
        }
        KeyCode::Escape => {
            if !query.is_empty() {
                state.edit.clear(query);
                commit_slot_query(shell, slot, query);
            } else {
                shell.close_search();
            }
        }
        KeyCode::KeyU if is_ctrl => {
            state.edit.clear(query);
            commit_slot_query(shell, slot, query);
        }
        _ => {
            let Some(character) = press.character else {
                return false;
            };
            if press.modifiers.control || press.modifiers.alt {
                return false;
            }
            state.edit.insert_char(query, character);
            commit_slot_query(shell, slot, query);
        }
    }
    true
}
