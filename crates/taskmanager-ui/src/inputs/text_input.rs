//! Single-line text input built on gpui's `EntityInputHandler` (absorption
//! §6.5): text/selection state, IME marked range, blink cursor, movement and
//! edit actions, validation with whole-string rollback, mask, and the
//! InputEvent protocol. No code-editor/LSP/search-panel surface (P3/P4 scope).
//!
//! Keymap: `text_input::init(cx)` registers the Linux edit keybindings under
//! the `Input` key context (mirroring the gc Linux keymap, trimmed).

use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use crate::{OptCallback, Validator};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Context, ElementId, ElementInputHandler, Entity, EventEmitter, FocusHandle,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, RenderOnce, SharedString, Styled,
    Subscription, Task, Window, actions, canvas, div, px,
};
use taskmanager_theme::Palette;
use taskmanager_theme::tokens;

mod handler;

/// Typed input events (absorption §6.1 event protocol).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// The value changed.
    Change,
    /// Enter was pressed in single-line mode.
    PressEnter { secondary: bool },
    /// The input gained focus.
    Focus,
    /// The input lost focus.
    Blur,
}

/// The `Input` key context name (bound by [`init`]).
pub const INPUT_CONTEXT: &str = "Input";

actions!(
    text_input,
    [
        MoveLeft,
        MoveRight,
        MoveToStart,
        MoveToEnd,
        SelectAll,
        DeleteBackward,
        DeleteForward,
        Copy,
        Cut,
        Paste,
        Enter,
        Escape,
    ]
);

/// Register the Linux single-line input keymap under the `Input` context.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", DeleteBackward, Some(INPUT_CONTEXT)),
        KeyBinding::new("delete", DeleteForward, Some(INPUT_CONTEXT)),
        KeyBinding::new("ctrl-a", SelectAll, Some(INPUT_CONTEXT)),
        KeyBinding::new("ctrl-c", Copy, Some(INPUT_CONTEXT)),
        KeyBinding::new("ctrl-x", Cut, Some(INPUT_CONTEXT)),
        KeyBinding::new("ctrl-v", Paste, Some(INPUT_CONTEXT)),
        KeyBinding::new("left", MoveLeft, Some(INPUT_CONTEXT)),
        KeyBinding::new("right", MoveRight, Some(INPUT_CONTEXT)),
        KeyBinding::new("home", MoveToStart, Some(INPUT_CONTEXT)),
        KeyBinding::new("end", MoveToEnd, Some(INPUT_CONTEXT)),
        KeyBinding::new("enter", Enter, Some(INPUT_CONTEXT)),
        KeyBinding::new("escape", Escape, Some(INPUT_CONTEXT)),
    ]);
}

/// Byte-based selection (start..end) with a reversed flag, mirroring the
/// UTF16Selection protocol but stored in bytes (absorption 6.2).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TextSelection {
    /// Byte range of the selection.
    pub range: Range<usize>,
    /// Whether the selection head is at the start of the range.
    pub reversed: bool,
}

impl TextSelection {
    /// A collapsed selection at `offset`.
    pub fn caret(offset: usize) -> Self {
        Self {
            range: offset..offset,
            reversed: false,
        }
    }

    /// The caret (head) offset.
    pub fn head(&self) -> usize {
        if self.reversed {
            self.range.start
        } else {
            self.range.end
        }
    }

    /// Move the caret to `offset`, collapsing the selection.
    pub fn collapse_to(&mut self, offset: usize) {
        self.range = offset..offset;
        self.reversed = false;
    }

    /// Set a non-collapsed selection.
    pub fn select(&mut self, range: Range<usize>, reversed: bool) {
        self.range = range;
        self.reversed = reversed;
    }
}

/// Cursor blink state machine (absorption 6.3-A, trimmed to single line).
pub struct BlinkCursor {
    visible: bool,
    paused: bool,
    epoch: u64,
    _task: Option<Task<()>>,
}

impl BlinkCursor {
    /// Create a blink cursor entity.
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            visible: true,
            paused: false,
            epoch: 0,
            _task: None,
        }
    }

    /// Pause blinking for the pause duration (called on every keystroke).
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.paused = true;
        self.visible = true;
        let this = cx.entity();
        let epoch = self.epoch;
        self._task = Some(cx.spawn(async move |_this, cx| {
            gpui::Timer::after(Duration::from_millis(300)).await;
            let _ = this.update(cx, |cursor, cx| {
                if cursor.epoch == epoch {
                    cursor.paused = false;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    /// Whether the caret should render this frame.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Advance one blink tick (called by the element on a timer).
    pub fn tick(&mut self, cx: &mut Context<Self>) {
        if self.paused {
            self.visible = true;
            return;
        }
        self.visible = !self.visible;
        cx.notify();
    }

    /// Bump the epoch, invalidating in-flight pause timers.
    pub fn bump_epoch(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
    }
}

/// Single-line text input state: the canonical value + selection live here.
pub struct TextInputState {
    focus_handle: FocusHandle,
    text: String,
    selection: TextSelection,
    ime_marked_range: Option<Range<usize>>,
    blink_cursor: Entity<BlinkCursor>,
    masked: bool,
    clean_on_escape: bool,
    disabled: bool,
    validate: Validator,
    placeholder: SharedString,
    focused: bool,
    _focus_subscriptions: Vec<Subscription>,
}

impl TextInputState {
    /// Create an empty input state.
    pub fn new(cx: &mut App) -> Self {
        let blink_cursor = cx.new(BlinkCursor::new);
        Self {
            focus_handle: cx.focus_handle(),
            text: String::new(),
            selection: TextSelection::caret(0),
            ime_marked_range: None,
            blink_cursor,
            masked: false,
            clean_on_escape: false,
            disabled: false,
            validate: None,
            placeholder: SharedString::default(),
            focused: false,
            _focus_subscriptions: Vec::new(),
        }
    }

    /// Whether the input currently has focus.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Wire focus in/out events onto the state so consumers can observe
    /// `InputEvent::Focus`/`Blur` without per-frame hooks. Called by the
    /// element render; subscriptions live on the state entity.
    pub fn observe_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self._focus_subscriptions.is_empty() {
            return;
        }
        let handle = self.focus_handle.clone();
        let this_in = cx.entity();
        let focus_in = window.on_focus_in(&handle, cx, move |_window, cx| {
            this_in.update(cx, |state, cx| {
                state.focused = true;
                cx.emit(InputEvent::Focus);
            });
        });
        let handle_out = self.focus_handle.clone();
        let this_out = cx.entity();
        let focus_out = window.on_focus_out(&handle_out, cx, move |_event, _window, cx| {
            this_out.update(cx, |state, cx| {
                state.focused = false;
                cx.emit(InputEvent::Blur);
            });
        });
        self._focus_subscriptions.push(focus_in);
        self._focus_subscriptions.push(focus_out);
    }

    /// The focus handle backing this input.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Current value.
    pub fn value(&self) -> SharedString {
        SharedString::from(if self.masked {
            "•".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        })
    }

    /// Current (masked) value for display.
    pub fn display_text(&self) -> &str {
        &self.text
    }

    /// Whether the input is masked (password-style).
    pub fn is_masked(&self) -> bool {
        self.masked
    }

    /// Set masking.
    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        self.masked = masked;
        cx.notify();
    }

    /// Whether Escape clears the input.
    pub fn clean_on_escape(&self) -> bool {
        self.clean_on_escape
    }

    /// Set Escape-cleans behavior.
    pub fn set_clean_on_escape(&mut self, clean: bool, cx: &mut Context<Self>) {
        self.clean_on_escape = clean;
        cx.notify();
    }

    /// Whether editing is disabled.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Set the disabled flag.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        cx.notify();
    }

    /// The placeholder shown when empty.
    pub fn placeholder(&self) -> SharedString {
        self.placeholder.clone()
    }

    /// Set the placeholder.
    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    /// Validation predicate (whole-string; failure rolls the edit back).
    pub fn set_validate(
        &mut self,
        validate: impl Fn(&str) -> bool + 'static,
        cx: &mut Context<Self>,
    ) {
        self.validate = Some(Rc::new(validate));
        cx.notify();
    }

    /// The caret (head) byte offset.
    pub fn caret(&self) -> usize {
        self.selection.head().min(self.text.len())
    }

    /// The selected byte range.
    pub fn selection(&self) -> TextSelection {
        self.selection.clone()
    }

    /// Whether the caret is at the given byte offset.
    pub fn caret_at(&self, offset: usize) -> bool {
        self.caret() == offset
    }

    /// Move the caret left by one character boundary.
    pub fn move_left(&mut self, cx: &mut Context<Self>) {
        let offset = previous_boundary(&self.text, self.caret());
        self.selection.collapse_to(offset);
        self.pause_blink(cx);
        cx.notify();
    }

    /// Move the caret right by one character boundary.
    pub fn move_right(&mut self, cx: &mut Context<Self>) {
        let offset = next_boundary(&self.text, self.caret());
        self.selection.collapse_to(offset);
        self.pause_blink(cx);
        cx.notify();
    }

    /// Move the caret to the start.
    pub fn move_to_start(&mut self, cx: &mut Context<Self>) {
        self.selection.collapse_to(0);
        self.pause_blink(cx);
        cx.notify();
    }

    /// Move the caret to the end.
    pub fn move_to_end(&mut self, cx: &mut Context<Self>) {
        self.selection.collapse_to(self.text.len());
        self.pause_blink(cx);
        cx.notify();
    }

    /// Select the whole value.
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selection.select(0..self.text.len(), false);
        self.pause_blink(cx);
        cx.notify();
    }

    /// Replace the selected range with `replacement` (or insert at caret when
    /// the selection is collapsed). Whole-string validation rolls back on
    /// failure (absorption 6.6-2).
    pub fn replace_selection(&mut self, replacement: &str, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let range = self.selection.range.clone();
        let mut pending = self.text.clone();
        pending.replace_range(range.clone(), replacement);
        if !self.is_valid(&pending) {
            return;
        }
        self.text = pending;
        let new_caret = range.start + replacement.len();
        self.selection.collapse_to(new_caret);
        self.ime_marked_range = None;
        self.pause_blink(cx);
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    fn is_valid(&self, pending: &str) -> bool {
        match &self.validate {
            Some(validate) => validate(pending),
            None => true,
        }
    }

    /// Delete one character before the caret (or the selection).
    pub fn delete_backward(&mut self, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let range = if self.selection.range.is_empty() {
            let end = self.caret();
            let start = previous_boundary(&self.text, end);
            start..end
        } else {
            self.selection.range.clone()
        };
        let mut pending = self.text.clone();
        pending.replace_range(range.clone(), "");
        if !self.is_valid(&pending) {
            return;
        }
        self.text = pending;
        self.selection.collapse_to(range.start);
        self.ime_marked_range = None;
        self.pause_blink(cx);
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    /// Delete one character after the caret (or the selection).
    pub fn delete_forward(&mut self, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let range = if self.selection.range.is_empty() {
            let start = self.caret();
            let end = next_boundary(&self.text, start);
            start..end
        } else {
            self.selection.range.clone()
        };
        let mut pending = self.text.clone();
        pending.replace_range(range.clone(), "");
        if !self.is_valid(&pending) {
            return;
        }
        self.text = pending;
        self.selection.collapse_to(range.start);
        self.ime_marked_range = None;
        self.pause_blink(cx);
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    /// Programmatic value set (no undo; caret to the end; scroll home).
    pub fn set_value(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        let value = value.into();
        if self.is_valid(&value) {
            self.text = value;
            self.selection.collapse_to(self.text.len());
            self.ime_marked_range = None;
            cx.emit(InputEvent::Change);
            cx.notify();
        }
    }

    /// Copy the selection (or the whole value when nothing selected) to the
    /// clipboard.
    pub fn copy(&self, cx: &mut App) {
        let slice = self.selected_or_all();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(slice));
    }

    /// Cut: copy then delete.
    pub fn cut(&mut self, cx: &mut Context<Self>) {
        let slice = self.selected_or_all();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(slice));
        self.delete_backward(cx);
    }

    /// Paste from the clipboard.
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = cx.read_from_clipboard()
            && let Some(text) = item.text()
        {
            self.replace_selection(text.as_str(), cx);
        }
    }

    /// Clear the value (Escape path when `clean_on_escape`).
    pub fn clean(&mut self, cx: &mut Context<Self>) {
        self.text.clear();
        self.selection.collapse_to(0);
        self.ime_marked_range = None;
        self.pause_blink(cx);
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    /// Focus the input (registers the window input handler).
    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    fn selected_or_all(&self) -> String {
        if self.selection.range.is_empty() {
            self.text.clone()
        } else {
            self.text[self.selection.range.clone()].to_string()
        }
    }

    fn pause_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_cursor.update(cx, |cursor, cx| cursor.pause(cx));
    }
}

/// The byte offset of the character boundary before `offset`.
#[must_use]
pub fn previous_boundary(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    if offset == 0 {
        return 0;
    }
    // Walk back over one full char; skip CR (absorption 6.6-1).
    let mut prev = offset;
    while prev > 0 {
        prev -= 1;
        if text.is_char_boundary(prev) {
            let ch = text[prev..offset].chars().next().unwrap_or('\u{fffd}');
            if ch != '\r' {
                return prev;
            }
        }
    }
    0
}

/// The byte offset of the character boundary after `offset`.
#[must_use]
pub fn next_boundary(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    if offset >= text.len() {
        return text.len();
    }
    let mut next = offset + 1;
    while next < text.len() && !text.is_char_boundary(next) {
        next += 1;
    }
    if text[offset..next]
        .chars()
        .next()
        .is_some_and(|ch| ch == '\r')
    {
        // Skip CRLF pair (absorption 6.6-1).
        let after = next + 1;
        next = after.min(text.len());
        while next < text.len() && !text.is_char_boundary(next) {
            next += 1;
        }
    }
    next
}

/// Event emitter for [`InputEvent`].
impl EventEmitter<InputEvent> for TextInputState {}

/// The rendered text input.
#[derive(IntoElement)]
pub struct TextInput {
    state: Entity<TextInputState>,
    palette: Palette,
    placeholder: Option<SharedString>,
    height: f32,
    radius: f32,
    on_change: OptCallback,
}

impl TextInput {
    /// Build a text input bound to `state`.
    pub fn new(state: Entity<TextInputState>, palette: Palette) -> Self {
        Self {
            state,
            palette,
            placeholder: None,
            height: 30.0,
            radius: 6.0,
            on_change: None,
        }
    }

    /// Placeholder shown while empty.
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Field height override (default 30px).
    #[must_use]
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Corner radius override (default 6px).
    #[must_use]
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Change handler fired after every edit.
    #[must_use]
    pub fn on_change(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for TextInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (disabled, focus_handle) = self
            .state
            .read_with(cx, |state, _| (state.disabled, state.focus_handle.clone()));
        let palette = self.palette;
        let placeholder = self.placeholder.or_else(|| {
            self.state
                .read_with(cx, |state, _| state.placeholder())
                .into()
        });
        let id = ElementId::named_usize(
            "tm-text-input",
            self.state.entity_id().as_non_zero_u64().get() as usize,
        );
        let state_entity = self.state.clone();
        let _on_change = self.on_change.clone();
        self.state
            .update(cx, |state, cx| state.observe_focus(window, cx));

        let focus_handle = focus_handle.clone().tab_stop(!disabled);

        let mut element = div()
            .id(id)
            .debug_selector(|| "tm-text-input".into())
            .key_context(INPUT_CONTEXT)
            .track_focus(&focus_handle)
            .flex()
            .items_center()
            .px(crate::theme_binding::definite_length(tokens::SPACE_10))
            .h(px(self.height))
            .rounded(px(self.radius))
            .bg(crate::theme_binding::fill(palette.surface))
            .border_1()
            .border_color(crate::theme_binding::hsla(palette.border))
            .text_sm()
            .text_color(crate::theme_binding::hsla(palette.fg))
            .focus(|style| style.border_color(crate::theme_binding::hsla(palette.ring)))
            .when(disabled, |el| el.opacity(0.5))
            .cursor_text()
            // Editing actions (Input context keymap above).
            .on_action({
                let state = state_entity.clone();
                move |_: &MoveLeft, _window, cx| {
                    state.update(cx, |state, cx| state.move_left(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &MoveRight, _window, cx| {
                    state.update(cx, |state, cx| state.move_right(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &MoveToStart, _window, cx| {
                    state.update(cx, |state, cx| state.move_to_start(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &MoveToEnd, _window, cx| {
                    state.update(cx, |state, cx| state.move_to_end(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &SelectAll, _window, cx| {
                    state.update(cx, |state, cx| state.select_all(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &DeleteBackward, _window, cx| {
                    state.update(cx, |state, cx| state.delete_backward(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &DeleteForward, _window, cx| {
                    state.update(cx, |state, cx| state.delete_forward(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &Copy, _window, cx| {
                    state.update(cx, |state, cx| state.copy(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &Cut, _window, cx| {
                    state.update(cx, |state, cx| state.cut(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &Paste, _window, cx| {
                    state.update(cx, |state, cx| state.paste(cx));
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &Enter, _window, cx| {
                    let secondary = false;
                    state.update(cx, |state, cx| {
                        let _ = state;
                        cx.emit(InputEvent::PressEnter { secondary });
                    });
                    // Propagate so a Dialog's Confirm can also react.
                    cx.propagate();
                }
            })
            .on_action({
                let state = state_entity.clone();
                move |_: &Escape, _window, cx| {
                    state.update(cx, |state, cx| {
                        if state.clean_on_escape() {
                            state.clean(cx);
                            true
                        } else {
                            false
                        }
                    });
                    // No clean -> let the overlay stack handle Escape.
                    cx.propagate();
                }
            });

        // Text/placeholder children (the caret is painted by the host canvas).
        let display = self
            .state
            .read_with(cx, |state, _| state.display_text().to_string());
        if display.is_empty() {
            element = element.child(
                div()
                    .text_color(crate::theme_binding::hsla(palette.fg_muted))
                    .child(placeholder.unwrap_or_default()),
            );
        } else {
            element = element.child(display);
        }

        // Register the platform input handler during paint: gpui drives
        // IME/text insertion through this bridge (absorption §6.2).
        let input_state = self.state.clone();
        element.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    window.handle_input(
                        &input_state.read(cx).focus_handle,
                        ElementInputHandler::new(bounds, input_state.clone()),
                        cx,
                    );
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }
}

#[cfg(test)]
#[path = "../../tests/gui/inputs/text_input.rs"]
mod tests;
