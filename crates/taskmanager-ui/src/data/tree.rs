//! Tree state machine for hierarchical data (absorption §2.1 `tree`, used
//! by the future process tree).
//!
//! Typed [`TreePath`]s address items from the root (one segment per depth);
//! the expansion state lives in a `HashSet<TreePath>` so a collapsed folder
//! cannot "leak" visible children — illegal states (selecting a hidden
//! entry, expanding a leaf) are unrepresentable by construction:
//! - [`TreeState::selected_index`] always refers to a visible flat entry;
//! - collapsing re-parents the selection onto the collapsed folder;
//! - [`TreeState::expand`] on a leaf is a no-op returning `false`.
//!
//! Deviations from gc `tree.rs`: `TreeItem` holds no shared
//! `Rc<RefCell>` expansion cell (the state owns expansion); Left on a
//! collapsed folder selects its parent (gc did nothing); selection skips
//! disabled entries.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, ElementId, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyBinding, ListSizingBehavior, MouseButton, MouseDownEvent, ParentElement, Pixels, RenderOnce,
    ScrollStrategy, SharedString, Styled, UniformListScrollHandle, Window, actions, div, px,
    uniform_list,
};

use taskmanager_theme::Palette;
use taskmanager_theme::tokens;

/// The tree key context (navigation bindings live under it).
pub const TREE_CONTEXT: &str = "TaskManagerTree";

actions!(
    tree,
    [
        TreeSelectUp,
        TreeSelectDown,
        TreeSelectLeft,
        TreeSelectRight,
        TreeConfirm,
    ]
);

/// Register the tree keymap (up/down move, left collapses / selects the
/// parent, right expands / selects the first child, Enter toggles).
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", TreeSelectUp, Some(TREE_CONTEXT)),
        KeyBinding::new("down", TreeSelectDown, Some(TREE_CONTEXT)),
        KeyBinding::new("left", TreeSelectLeft, Some(TREE_CONTEXT)),
        KeyBinding::new("right", TreeSelectRight, Some(TREE_CONTEXT)),
        KeyBinding::new("enter", TreeConfirm, Some(TREE_CONTEXT)),
    ]);
}

/// A typed path into the tree: one segment per depth; children are
/// addressed as [`TreePath::child`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct TreePath(Vec<usize>);

impl TreePath {
    /// The empty path (base for root children).
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Build a path from explicit segments (root to leaf).
    pub fn from_segments(segments: &[usize]) -> Self {
        Self(segments.to_vec())
    }

    /// The path segments.
    pub fn segments(&self) -> &[usize] {
        &self.0
    }

    /// The depth of this path (number of segments).
    pub fn depth(&self) -> usize {
        self.0.len()
    }

    /// The path of the `ix`-th child of this path.
    #[must_use]
    pub fn child(&self, ix: usize) -> Self {
        let mut segments = self.0.clone();
        segments.push(ix);
        Self(segments)
    }

    /// The parent path, if any.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            let mut segments = self.0.clone();
            segments.pop();
            Some(Self(segments))
        }
    }
}

/// One tree item (configured content + children). Expansion *state* is
/// owned by [`TreeState`]; `expanded` here is the initial hint applied when
/// the items are loaded.
#[derive(Clone, Debug)]
pub struct TreeItem {
    /// Stable identity (e.g. the full process path).
    pub id: SharedString,
    /// The display label.
    pub label: SharedString,
    children: Vec<TreeItem>,
    initially_expanded: bool,
    disabled: bool,
}

impl TreeItem {
    /// Create an item with a stable identity and display label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            children: Vec::new(),
            initially_expanded: false,
            disabled: false,
        }
    }

    /// Append one child.
    #[must_use]
    pub fn child(mut self, child: TreeItem) -> Self {
        self.children.push(child);
        self
    }

    /// Append several children.
    #[must_use]
    pub fn children(mut self, children: impl IntoIterator<Item = TreeItem>) -> Self {
        self.children.extend(children);
        self
    }

    /// Set the initial expansion hint (folders only; ignored for leaves).
    #[must_use]
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.initially_expanded = expanded;
        self
    }

    /// Mark the item disabled (not selectable/activatable).
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Whether this item has children.
    pub fn is_folder(&self) -> bool {
        !self.children.is_empty()
    }

    /// Whether this item is disabled.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// The configured children.
    pub fn child_items(&self) -> &[TreeItem] {
        &self.children
    }

    fn is_initially_expanded(&self) -> bool {
        self.initially_expanded
    }
}

/// One flattened, visible entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeEntry {
    /// The typed path of this entry.
    pub path: TreePath,
    /// The depth (root children are 0).
    pub depth: usize,
}

impl TreeEntry {
    /// The typed path of this entry.
    pub fn path(&self) -> &TreePath {
        &self.path
    }

    /// The depth of this entry.
    pub fn depth(&self) -> usize {
        self.depth
    }
}

/// The tree state: roots + expansion set + flattened entries + selection.
pub struct TreeState {
    focus_handle: FocusHandle,
    roots: Vec<TreeItem>,
    expanded: HashSet<TreePath>,
    entries: Vec<TreeEntry>,
    selected: Option<usize>,
    scroll_handle: UniformListScrollHandle,
}

impl TreeState {
    /// Create an empty tree.
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle().tab_stop(true),
            roots: Vec::new(),
            expanded: HashSet::new(),
            entries: Vec::new(),
            selected: None,
            scroll_handle: UniformListScrollHandle::new(),
        }
    }

    /// Replace the tree content. The expansion state resets to the items'
    /// initial hints; the selection is cleared.
    pub fn set_items(&mut self, roots: Vec<TreeItem>, cx: &mut Context<Self>) {
        self.roots = roots;
        self.expanded = self
            .roots
            .iter()
            .enumerate()
            .flat_map(|(ix, item)| collect_initial_expanded(&TreePath::new().child(ix), item))
            .collect();
        self.selected = None;
        self.rebuild_entries();
        cx.notify();
    }

    /// The flattened visible entries.
    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }

    /// The item at a typed path, if the path addresses a real item.
    pub fn item_for_path(&self, path: &TreePath) -> Option<&TreeItem> {
        let mut items: &[TreeItem] = &self.roots;
        let mut item = None;
        for segment in path.segments() {
            item = items.get(*segment);
            items = item?.child_items();
        }
        item
    }

    /// The entry and item at a flat visible index.
    pub fn entry_and_item(&self, ix: usize) -> Option<(&TreeEntry, &TreeItem)> {
        let entry = self.entries.get(ix)?;
        let item = self.item_for_path(&entry.path)?;
        Some((entry, item))
    }

    /// The currently selected flat index, if any (always a visible entry).
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// The typed path of the selection, if any.
    pub fn selected_path(&self) -> Option<&TreePath> {
        self.selected
            .and_then(|ix| self.entries.get(ix))
            .map(|e| &e.path)
    }

    /// Select a visible flat index (or `None` to clear). Out-of-range
    /// requests are clamped instead of creating an illegal state.
    pub fn set_selected_index(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        self.selected = match ix {
            Some(ix) if !self.entries.is_empty() => Some(ix.min(self.entries.len() - 1)),
            _ => None,
        };
        cx.notify();
    }

    /// Whether the folder at `path` is currently expanded. Leaves always
    /// report `false` (there is nothing to expand).
    pub fn is_expanded(&self, path: &TreePath) -> bool {
        self.item_for_path(path).is_some_and(TreeItem::is_folder) && self.expanded.contains(path)
    }

    /// Expand the folder at `path`. Returns `false` for leaves, missing
    /// paths, disabled items, or folders already expanded (no-op).
    pub fn expand(&mut self, path: &TreePath, cx: &mut Context<Self>) -> bool {
        let Some(item) = self.item_for_path(path) else {
            return false;
        };
        if !item.is_folder() || item.is_disabled() || self.expanded.contains(path) {
            return false;
        }
        self.expanded.insert(path.clone());
        self.rebuild_entries();
        cx.notify();
        true
    }

    /// Collapse the folder at `path`. Returns `false` when there is
    /// nothing to collapse.
    pub fn collapse(&mut self, path: &TreePath, cx: &mut Context<Self>) -> bool {
        if self.expanded.remove(path) {
            self.rebuild_entries();
            cx.notify();
            true
        } else {
            false
        }
    }

    /// Toggle the folder at `path`. Returns `false` for leaves/missing
    /// paths (nothing changed).
    pub fn toggle(&mut self, path: &TreePath, cx: &mut Context<Self>) -> bool {
        if self.is_expanded(path) {
            self.collapse(path, cx)
        } else {
            self.expand(path, cx)
        }
    }

    /// Move the selection one step (wrapping), skipping disabled entries.
    pub fn move_selection(&mut self, direction: MoveDirection, cx: &mut Context<Self>) {
        let count = self.entries.len();
        if count == 0 {
            return;
        }
        let mut ix = match self.selected {
            Some(ix) => match direction {
                MoveDirection::Down => (ix + 1) % count,
                MoveDirection::Up => (ix + count - 1) % count,
            },
            None => match direction {
                MoveDirection::Down => 0,
                MoveDirection::Up => count - 1,
            },
        };
        // Bounded scan skipping disabled entries.
        for _ in 0..count {
            if self.item_at(ix).is_some_and(|item| !item.is_disabled()) {
                self.selected = Some(ix);
                self.scroll_handle.scroll_to_item(ix, ScrollStrategy::Top);
                cx.notify();
                return;
            }
            ix = match direction {
                MoveDirection::Down => (ix + 1) % count,
                MoveDirection::Up => (ix + count - 1) % count,
            };
        }
    }

    /// Keyboard Left: collapse an expanded folder; otherwise select its
    /// parent (deviation from gc, which did nothing).
    pub fn select_left(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.selected else { return };
        let Some((entry, _item)) = self.entry_and_item(ix) else {
            return;
        };
        let path = entry.path.clone();
        if self.is_expanded(&path) {
            self.collapse(&path, cx);
            return;
        }
        if let Some(parent) = path.parent()
            && let Some(parent_ix) = self.entries.iter().position(|e| e.path == parent)
        {
            self.selected = Some(parent_ix);
            self.scroll_handle
                .scroll_to_item(parent_ix, ScrollStrategy::Top);
            cx.notify();
        }
    }

    /// Keyboard Right: expand a collapsed folder; otherwise select its
    /// first visible child.
    pub fn select_right(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.selected else { return };
        let Some((entry, _item)) = self.entry_and_item(ix) else {
            return;
        };
        let path = entry.path.clone();
        let depth = entry.depth;
        if !self.is_expanded(&path) && self.expand(&path, cx) {
            return;
        }
        // Select the first visible child (the next entry at depth+1).
        let has_visible_child = self.entries.get(ix + 1).is_some_and(|e| e.depth > depth);
        if has_visible_child {
            let next_ix = ix + 1;
            self.selected = Some(next_ix);
            self.scroll_handle
                .scroll_to_item(next_ix, ScrollStrategy::Top);
            cx.notify();
        }
    }

    /// Keyboard Enter: toggle the selected folder.
    pub fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.selected else { return };
        let Some((entry, _item)) = self.entry_and_item(ix) else {
            return;
        };
        let path = entry.path.clone();
        let _ = self.toggle(&path, cx);
    }

    /// Pointer click on a visible entry: select it and toggle folders
    /// (gc parity).
    pub fn on_entry_click(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some((entry, _item)) = self.entry_and_item(ix) else {
            return;
        };
        let path = entry.path.clone();
        self.selected = Some(ix);
        let _ = self.toggle(&path, cx);
        cx.notify();
    }

    /// The scroll handle (for scrollbars / programmatic scrolling).
    pub fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll_handle
    }

    fn item_at(&self, ix: usize) -> Option<&TreeItem> {
        self.entry_and_item(ix).map(|(_, item)| item)
    }

    fn rebuild_entries(&mut self) {
        let previous = self.selected_path().cloned();
        let mut entries = Vec::new();
        for (ix, root) in self.roots.iter().enumerate() {
            let path = TreePath::new().child(ix);
            flatten_item(root, &path, 0, &self.expanded, &mut entries);
        }
        self.entries = entries;
        // Re-parent or clamp the selection: it must always address a
        // visible entry (illegal states unrepresentable).
        self.selected = match previous {
            Some(path) => self
                .entries
                .iter()
                .position(|e| e.path == path)
                // The path collapsed away: select its deepest visible
                // ancestor.
                .or_else(|| {
                    let mut ancestor = path.parent();
                    while let Some(p) = ancestor {
                        if let Some(ix) = self.entries.iter().position(|e| e.path == p) {
                            return Some(ix);
                        }
                        ancestor = p.parent();
                    }
                    None
                })
                .or_else(|| {
                    if self.entries.is_empty() {
                        None
                    } else {
                        Some(self.entries.len() - 1)
                    }
                }),
            None => None,
        };
    }
}

/// Collect the initial expansion state of a subtree into `out`.
fn collect_initial_expanded(path: &TreePath, item: &TreeItem) -> Vec<TreePath> {
    let mut out = Vec::new();
    if item.is_folder() && item.is_initially_expanded() {
        out.push(path.clone());
        for (ix, child) in item.child_items().iter().enumerate() {
            out.extend(collect_initial_expanded(&path.child(ix), child));
        }
    }
    out
}

/// Flatten one subtree (depth-first) into `out`.
fn flatten_item(
    item: &TreeItem,
    path: &TreePath,
    depth: usize,
    expanded: &HashSet<TreePath>,
    out: &mut Vec<TreeEntry>,
) {
    out.push(TreeEntry {
        path: path.clone(),
        depth,
    });
    if item.is_folder() && expanded.contains(path) {
        for (ix, child) in item.child_items().iter().enumerate() {
            flatten_item(child, &path.child(ix), depth + 1, expanded, out);
        }
    }
}

/// Selection traversal direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveDirection {
    /// Toward the end of the flattened list.
    Down,
    /// Toward the start of the flattened list.
    Up,
}

/// Row renderer for one flattened tree entry.
pub type TreeRowRenderer =
    Rc<dyn Fn(usize, &TreeEntry, &TreeItem, bool, &mut Window, &mut App) -> AnyElement>;

/// The rendered tree: binds the state to the key context and renders the
/// flattened entries in a uniform list with the default row (indent +
/// disclosure + label).
#[derive(IntoElement)]
pub struct Tree {
    state: Entity<TreeState>,
    palette: Palette,
    row_height: Pixels,
    render_item: TreeRowRenderer,
}

impl Tree {
    /// Build a tree with the default row renderer.
    pub fn new(state: &Entity<TreeState>, palette: Palette) -> Self {
        let render_state = state.clone();
        Self {
            state: state.clone(),
            palette,
            row_height: px(24.0),
            render_item: Rc::new(move |_ix, entry, item, selected, _window, cx| {
                let expanded = render_state.read_with(cx, |s, _| s.is_expanded(&entry.path));
                Self::default_row(entry, item, expanded, selected, palette, cx).into_any_element()
            }),
        }
    }

    /// Build a tree with a custom row renderer (receives the flat index,
    /// the entry, the item, and the selected flag).
    pub fn with_renderer<R, E>(state: &Entity<TreeState>, palette: Palette, render_item: R) -> Self
    where
        R: Fn(usize, &TreeEntry, &TreeItem, bool, &mut Window, &mut App) -> E + 'static,
        E: IntoElement,
    {
        Self {
            state: state.clone(),
            palette,
            row_height: px(24.0),
            render_item: Rc::new(move |ix, entry, item, selected, window, cx| {
                render_item(ix, entry, item, selected, window, cx).into_any_element()
            }),
        }
    }

    /// Uniform row height (default 24px).
    #[must_use]
    pub fn row_height(mut self, row_height: impl Into<Pixels>) -> Self {
        self.row_height = row_height.into();
        self
    }

    /// The built-in row: depth indent + disclosure glyph + label.
    pub fn default_row(
        entry: &TreeEntry,
        item: &TreeItem,
        expanded: bool,
        selected: bool,
        palette: Palette,
        _cx: &mut App,
    ) -> impl IntoElement {
        let indent = px(12.0) * entry.depth as f32;
        let _ = selected;
        div()
            .h_full()
            .pl(px(4.0) + indent)
            .pr(tokens::SPACE_8)
            .flex()
            .flex_row()
            .items_center()
            .gap(tokens::SPACE_4)
            .child(
                div()
                    .w(px(16.0))
                    .text_color(palette.fg_muted)
                    .child(if item.is_folder() {
                        if expanded { "▾" } else { "▸" }
                    } else {
                        ""
                    }),
            )
            .child(
                div()
                    .text_color(if item.is_disabled() {
                        palette.fg_muted
                    } else {
                        palette.fg
                    })
                    .child(item.label.clone()),
            )
    }
}

impl RenderOnce for Tree {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self
            .state
            .read_with(cx, |state, _| state.focus_handle.clone());
        let scroll_handle = self
            .state
            .read_with(cx, |state, _| state.scroll_handle.clone());
        let render_item = self.render_item.clone();
        let state = self.state.clone();
        let palette = self.palette;
        let row_height = self.row_height;

        div()
            .id(ElementId::named_usize(
                "tm-tree",
                self.state.entity_id().as_non_zero_u64().get() as usize,
            ))
            .debug_selector(|| "tm-tree".into())
            .size_full()
            .key_context(TREE_CONTEXT)
            .track_focus(&focus_handle)
            .on_action({
                let state = self.state.clone();
                move |_: &TreeSelectUp, _w, cx| {
                    state.update(cx, |state, cx| state.move_selection(MoveDirection::Up, cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &TreeSelectDown, _w, cx| {
                    state.update(cx, |state, cx| {
                        state.move_selection(MoveDirection::Down, cx)
                    });
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &TreeSelectLeft, _w, cx| {
                    state.update(cx, |state, cx| state.select_left(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &TreeSelectRight, _w, cx| {
                    state.update(cx, |state, cx| state.select_right(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &TreeConfirm, _w, cx| {
                    state.update(cx, |state, cx| state.confirm(cx));
                }
            })
            .child(
                uniform_list(
                    "tree-entries",
                    self.state.read_with(cx, |state, _| state.entries.len()),
                    {
                        let state = state.clone();
                        let render_item = render_item.clone();
                        move |visible_range: Range<usize>, window, cx| {
                            let entries: Vec<(TreeEntry, TreeItem)> =
                                state.read_with(cx, |state, _| {
                                    visible_range
                                        .clone()
                                        .filter_map(|ix| {
                                            state
                                                .entry_and_item(ix)
                                                .map(|(e, i)| (e.clone(), i.clone()))
                                        })
                                        .collect::<Vec<(TreeEntry, TreeItem)>>()
                                });
                            let selected = state.read_with(cx, |s, _| s.selected);
                            let mut items = Vec::with_capacity(entries.len());
                            for (ix, (entry, item)) in entries.iter().enumerate() {
                                let flat_ix = visible_range.start + ix;
                                let row = render_item(
                                    flat_ix,
                                    entry,
                                    item,
                                    selected == Some(flat_ix),
                                    window,
                                    cx,
                                );
                                items.push(
                                    div()
                                        .id(flat_ix)
                                        .h(row_height)
                                        .when(selected == Some(flat_ix), |this| {
                                            this.bg(crate::styled::hover_fill(palette.surface))
                                        })
                                        .when(!item.is_disabled(), |this| {
                                            this.on_mouse_down(MouseButton::Left, {
                                                let state = state.clone();
                                                move |_e: &MouseDownEvent, _w, cx| {
                                                    state.update(cx, |state, cx| {
                                                        state.on_entry_click(flat_ix, cx)
                                                    });
                                                }
                                            })
                                        })
                                        .child(row),
                                );
                            }
                            items
                        }
                    },
                )
                .size_full()
                .flex_grow()
                .track_scroll(scroll_handle)
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .into_any_element(),
            )
    }
}

#[cfg(test)]
#[path = "../../tests/gui/data/tree.rs"]
mod tests;
