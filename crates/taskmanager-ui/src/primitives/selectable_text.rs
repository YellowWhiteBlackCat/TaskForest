//! Read-only text selection and clipboard behavior.
//!
//! GPUI's plain `StyledText` exposes precise UTF-8 index/position mapping but
//! does not own selection state. This primitive adds that missing interaction
//! without turning read-only values into editable text fields.

use std::collections::HashMap;
use std::ops::Range;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClipboardItem, Context, CursorStyle, DispatchPhase, Element, ElementId, FocusHandle,
    Global, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior, InspectorElementId,
    InteractiveElement, IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, RenderOnce, SharedString, Styled, StyledText,
    Subscription, WeakEntity, Window, WindowId, actions, div,
};
use taskmanager_theme::Palette;

/// Key context for focused read-only selectable text.
pub const SELECTABLE_TEXT_CONTEXT: &str = "TaskManagerSelectableText";

actions!(selectable_text, [CopySelection, SelectAllText]);

#[derive(Default)]
struct SelectionRegistry {
    active_by_window: HashMap<WindowId, WeakEntity<SelectableTextState>>,
    window_closed_subscription: Option<Subscription>,
}

impl Global for SelectionRegistry {}

impl SelectionRegistry {
    fn prune_released(&mut self) {
        self.active_by_window
            .retain(|_, state| state.upgrade().is_some());
    }
}

/// Register cross-platform selection-copy bindings.
pub fn init(cx: &mut App) {
    if !cx.has_global::<SelectionRegistry>() {
        cx.set_global(SelectionRegistry::default());
        // GPUI invokes this after the window has left its registry. Defer the
        // weak-reference sweep once more so the closed window's keyed element
        // state has also released its final strong entity handle.
        let subscription = cx.on_window_closed(|cx| {
            cx.defer(|cx| {
                cx.global_mut::<SelectionRegistry>().prune_released();
            });
        });
        cx.global_mut::<SelectionRegistry>()
            .window_closed_subscription = Some(subscription);
    }
    cx.bind_keys([
        KeyBinding::new("ctrl-c", CopySelection, Some(SELECTABLE_TEXT_CONTEXT)),
        KeyBinding::new("ctrl-a", SelectAllText, Some(SELECTABLE_TEXT_CONTEXT)),
        KeyBinding::new("cmd-c", CopySelection, Some(SELECTABLE_TEXT_CONTEXT)),
        KeyBinding::new("cmd-a", SelectAllText, Some(SELECTABLE_TEXT_CONTEXT)),
    ]);
}

fn activate_selection(
    window_id: WindowId,
    state: &gpui::Entity<SelectableTextState>,
    cx: &mut App,
) {
    let current = state.downgrade();
    let previous = {
        let registry = cx.global_mut::<SelectionRegistry>();
        // Also sweep on every activation. This is the fail-safe for hosts that
        // destroy a keyed state without delivering a window-close callback.
        registry.prune_released();
        registry.active_by_window.insert(window_id, current.clone())
    };
    if let Some(previous) = previous
        && previous != current
    {
        let _ = previous.update(cx, |state, cx| state.clear(cx));
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_selectable_text_registry_tests.rs"]
mod registry_tests;

fn owns_window_selection(
    window_id: WindowId,
    state: &gpui::Entity<SelectableTextState>,
    cx: &App,
) -> bool {
    let current = state.downgrade();
    cx.global::<SelectionRegistry>()
        .active_by_window
        .get(&window_id)
        .is_some_and(|active| active == &current)
}

fn write_pointer_selection(text: String, cx: &mut App) {
    let item = ClipboardItem::new_string(text);
    #[cfg(target_os = "linux")]
    cx.write_to_primary(item);
    #[cfg(not(target_os = "linux"))]
    cx.write_to_clipboard(item);
}

struct SelectableTextState {
    focus_handle: FocusHandle,
    text: String,
    anchor: Option<usize>,
    selection: Range<usize>,
}

impl SelectableTextState {
    fn new(text: String, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text,
            anchor: None,
            selection: 0..0,
        }
    }

    fn sync_text(&mut self, text: &str) {
        if self.text != text {
            self.text.clear();
            self.text.push_str(text);
            self.anchor = None;
            self.selection = 0..0;
        }
    }

    fn begin(&mut self, index: usize, click_count: usize, cx: &mut Context<Self>) {
        let index = char_boundary_at_or_before(&self.text, index);
        match click_count {
            0 | 1 => {
                self.anchor = Some(index);
                self.selection = index..index;
            }
            2 => {
                self.anchor = None;
                self.selection = word_range(&self.text, index);
            }
            _ => {
                self.anchor = None;
                self.selection = 0..self.text.len();
            }
        }
        cx.notify();
    }

    fn extend(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(anchor) = self.anchor else {
            return;
        };
        let head = char_boundary_at_or_before(&self.text, index);
        self.selection = anchor.min(head)..anchor.max(head);
        cx.notify();
    }

    fn finish(&mut self, cx: &mut Context<Self>) -> Option<String> {
        self.anchor = None;
        cx.notify();
        self.selected_text()
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.anchor = None;
        self.selection = 0..self.text.len();
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.anchor = None;
        self.selection = 0..0;
        cx.notify();
    }

    fn selected_text(&self) -> Option<String> {
        (!self.selection.is_empty()).then(|| self.text[self.selection.clone()].to_owned())
    }
}

fn char_boundary_at_or_before(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn word_range(text: &str, index: usize) -> Range<usize> {
    if text.is_empty() {
        return 0..0;
    }
    let mut index = char_boundary_at_or_before(text, index);
    if index == text.len() {
        index = text.char_indices().next_back().map_or(0, |(ix, _)| ix);
    }
    let category = text[index..]
        .chars()
        .next()
        .map(word_character)
        .unwrap_or(false);
    let mut start = index;
    while let Some((previous, ch)) = text[..start].char_indices().next_back() {
        if word_character(ch) != category {
            break;
        }
        start = previous;
    }
    let mut end = index;
    for (offset, ch) in text[index..].char_indices() {
        if word_character(ch) != category {
            break;
        }
        end = index + offset + ch.len_utf8();
    }
    start..end
}

fn word_character(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Read-only text with native drag selection and clipboard shortcuts.
#[derive(IntoElement)]
pub struct SelectableText {
    id: ElementId,
    text: SharedString,
    palette: Palette,
    single_line: bool,
    debug_selector: Option<SharedString>,
    selected_debug_selector: Option<SharedString>,
}

impl SelectableText {
    /// Build selectable text with a stable identity.
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>, palette: Palette) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            palette,
            single_line: false,
            debug_selector: None,
            selected_debug_selector: None,
        }
    }

    /// Keep a bounded readout on one visual line with an ellipsis.
    ///
    /// Selection still owns the complete source string: pointer ranges and
    /// Ctrl/Cmd+A copy are not shortened to the painted ellipsis. Long-form
    /// detail values remain wrapping by default and must opt in explicitly.
    #[must_use]
    pub fn single_line(mut self) -> Self {
        self.single_line = true;
        self
    }

    /// Attach a geometry selector for behavior tests.
    #[must_use]
    pub fn debug_selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.debug_selector = Some(selector.into());
        self
    }

    /// Attach an optional marker that exists only while this text owns a
    /// non-empty selection. Intended for cross-instance behavior probes.
    #[must_use]
    pub fn selected_debug_selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.selected_debug_selector = Some(selector.into());
        self
    }
}

impl RenderOnce for SelectableText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state_key = ElementId::from((self.id.clone(), "state"));
        let initial_text = self.text.to_string();
        let state = window.use_keyed_state(state_key, cx, |_window, cx| {
            SelectableTextState::new(initial_text, cx)
        });
        let (focus_handle, mut selection) = state.update(cx, |state, _cx| {
            state.sync_text(&self.text);
            (state.focus_handle.clone(), state.selection.clone())
        });
        let window_id = window.window_handle().window_id();
        if !owns_window_selection(window_id, &state, cx) {
            selection = 0..0;
        }
        let selected = !selection.is_empty();

        let mut styled = StyledText::new(self.text.clone());
        if !selection.is_empty() {
            styled = styled.with_highlights([(
                selection,
                HighlightStyle {
                    background_color: Some(self.palette.selection.into()),
                    ..Default::default()
                },
            )]);
        }

        let copy_state = state.clone();
        let select_all_state = state.clone();
        let release_state = state.clone();
        let selector = self.debug_selector;
        let selected_selector = self.selected_debug_selector;
        let single_line = self.single_line;
        let select_all_window_id = window_id;
        let select_all_owner = state.clone();
        let mut root = div()
            .id(self.id.clone())
            .track_focus(&focus_handle)
            .key_context(SELECTABLE_TEXT_CONTEXT)
            .cursor_text()
            .min_w(gpui::px(0.0))
            .when(single_line, |text| text.w_full().truncate())
            .when_some(selector, |text, selector| {
                text.debug_selector(move || selector.to_string())
            })
            .on_action(move |_: &CopySelection, _window, cx| {
                if let Some(text) = copy_state.read_with(cx, |state, _cx| state.selected_text()) {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                cx.stop_propagation();
            })
            .on_action(move |_: &SelectAllText, _window, cx| {
                activate_selection(select_all_window_id, &select_all_owner, cx);
                select_all_state.update(cx, |state, cx| state.select_all(cx));
                cx.stop_propagation();
            })
            .on_mouse_up_out(MouseButton::Left, move |_event, _window, cx| {
                let selected = release_state.update(cx, |state, cx| state.finish(cx));
                if let Some(text) = selected {
                    write_pointer_selection(text, cx);
                }
            })
            .child(SelectableTextElement {
                id: ElementId::from((self.id, "text")),
                text: styled,
                state,
                focus_handle,
            });
        if let Some(selector) = selected_selector {
            root = root.child(
                div()
                    .absolute()
                    .size(gpui::px(if selected { 1.0 } else { 0.0 }))
                    .debug_selector(move || selector.to_string()),
            );
        }
        root
    }
}

struct SelectableTextElement {
    id: ElementId,
    text: StyledText,
    state: gpui::Entity<SelectableTextState>,
    focus_handle: FocusHandle,
}

impl IntoElement for SelectableTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableTextElement {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(None, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.text
            .prepaint(None, inspector_id, bounds, state, window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        _bounds: gpui::Bounds<gpui::Pixels>,
        request_state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.text.layout().clone();
        if hitbox.is_hovered(window) {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }

        let down_hitbox = hitbox.clone();
        let down_layout = layout.clone();
        let down_state = self.state.clone();
        let focus_handle = self.focus_handle.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && down_hitbox.is_hovered(window)
            {
                let index = down_layout
                    .index_for_position(event.position)
                    .unwrap_or_else(|index| index);
                focus_handle.focus(window);
                activate_selection(window.window_handle().window_id(), &down_state, cx);
                down_state.update(cx, |state, cx| state.begin(index, event.click_count, cx));
                cx.stop_propagation();
            }
        });

        let move_layout = layout.clone();
        let move_state = self.state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase == DispatchPhase::Bubble
                && event.dragging()
                && move_state.read_with(cx, |state, _cx| state.anchor.is_some())
            {
                let index = move_layout
                    .index_for_position(event.position)
                    .unwrap_or_else(|index| index);
                move_state.update(cx, |state, cx| state.extend(index, cx));
                cx.stop_propagation();
            }
        });

        let up_state = self.state.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                let selected = up_state.update(cx, |state, cx| state.finish(cx));
                if let Some(text) = selected {
                    write_pointer_selection(text, cx);
                }
            }
        });

        self.text.paint(
            None,
            inspector_id,
            _bounds,
            request_state,
            &mut (),
            window,
            cx,
        );
    }
}
