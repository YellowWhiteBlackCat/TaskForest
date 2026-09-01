//! Shared modal, tooltip and elastic-text overlay elements.

use gpui::{
    AnimationExt, AnyElement, App, AppContext, Div, Entity, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Styled, Window, anchored, deferred,
    div, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_theme::tokens;
use taskmanager_theme::{Theme, WindowCorner};
use taskmanager_ui::overlays::dialog::Dialog;
use taskmanager_ui::overlays::layer_stack::{LayerBackfill, LayerStack};
use taskmanager_ui::theme_binding::{appear, fade_in};

/// A modal overlay built on the own `taskmanager_ui` dialog + layer stack
/// (`overlays::dialog::Dialog` pushed into a [`LayerStack`] modal layer).
///
/// This is the chrome wrapper for the Settings modal (root.rs), the CPU-affinity
/// modal (processes_view.rs) and every confirmation dialog. It renders:
///   * a full-window **mask** (scrim) that dismisses on outside-click, and
///   * the **Dialog** panel, which supplies the bg/border/radius/shadow, the
///     title header, the close (X) button, the modal focus trap and the
///     ESC/Enter keyboard paths.
///
/// `content` is the Dialog's child — pass the modal BODY (sections / chip grid)
/// WITHOUT its own outer chrome box; the Dialog IS the chrome wrapper.
///
/// # Hosting
/// The stateless helper (no RootView handle, fixed call-site signature) hosts
/// its own per-dialog [`LayerStack`] entity, keyed by the call site's code
/// location via `Window::use_state` — one stack per dialog element. The modal
/// is pushed on the first render the dialog is open and closed through the
/// layer's close paths (X / mask / ESC / footer buttons), which restore the
/// pre-modal trigger via `focus::restore_modal`. The caller's `on_close` runs
/// on every close path (mask + X + ESC + buttons), so the "open" flag the
/// caller owns is always cleared and the layer is never re-pushed.
///
/// Title/content are stored as per-frame slots: the caller rebuilds them every
/// render (live settings / search values), and the layer's content builder
/// re-invokes the dialog render each frame, so the modal stays live — the same
/// refresh behavior the old stateless gc Dialog element had.
///
/// `#[track_caller]` chains the host key through to the call site.
#[track_caller]
pub fn dialog_overlay(
    t: &Theme,
    window: &mut Window,
    cx: &mut App,
    title: impl IntoElement,
    on_close: impl Fn(&mut Window, &mut App) + Clone + 'static,
    content: impl IntoElement,
) -> impl IntoElement {
    dialog_overlay_width(t, window, cx, px(480.0), title, on_close, content)
}

/// The dialog close handler boxed once, shared by every close path.
type CloseHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// Per-dialog overlay host state: the layer stack plus per-frame title/content
/// slots refreshed by the caller's element build before the stack renders.
struct DialogOverlayHost {
    stack: Entity<LayerStack>,
    title: Rc<RefCell<Option<AnyElement>>>,
    content: Rc<RefCell<Option<AnyElement>>>,
}

impl DialogOverlayHost {
    fn new(cx: &mut App) -> Self {
        Self {
            stack: cx.new(|_| LayerStack::new()),
            title: Rc::new(RefCell::new(None)),
            content: Rc::new(RefCell::new(None)),
        }
    }
}

/// Take the latest caller-built element out of a per-frame slot (empty slot →
/// an empty div, so a layer whose slot was never written still renders).
fn take_slot(slot: Rc<RefCell<Option<AnyElement>>>) -> AnyElement {
    slot.borrow_mut()
        .take()
        .unwrap_or_else(|| div().into_any_element())
}

/// Dialog overlay with an explicit panel width. Use this when a responsive
/// body intentionally renders multiple columns; the standard wrapper remains
/// 480 px for confirmations and compact forms.
///
/// `#[track_caller]` chains through to the call site so `Window::use_state`
/// keys each dialog's host by its own code location (multiple dialogs may be
/// open at once; each keeps its own stack).
#[track_caller]
pub fn dialog_overlay_width(
    t: &Theme,
    window: &mut Window,
    cx: &mut App,
    width: Pixels,
    title: impl IntoElement,
    on_close: impl Fn(&mut Window, &mut App) + Clone + 'static,
    content: impl IntoElement,
) -> impl IntoElement {
    let host = window.use_state(cx, |_window, cx| DialogOverlayHost::new(cx));
    // Refresh the per-frame slots BEFORE the stack renders: the caller rebuilt
    // this frame's title/content, and the layer's content builder reads them.
    let slots = host.read(cx);
    *slots.title.borrow_mut() = Some(title.into_any_element());
    *slots.content.borrow_mut() = Some(content.into_any_element());
    let title_slot = slots.title.clone();
    let content_slot = slots.content.clone();
    let stack = slots.stack.clone();
    let _ = slots;

    let palette = t.palette();
    let on_close: CloseHandler = Rc::new(on_close);
    let on_close_for_dialog = on_close.clone();
    let on_close_for_scrim = on_close;
    let dialog = Dialog::new()
        .palette(palette)
        .w(f32::from(width))
        .title(Rc::new(move |_window, _cx| take_slot(title_slot.clone())))
        .content(Rc::new(move |_window, _cx| take_slot(content_slot.clone())))
        .on_close(move |window, cx| (on_close_for_dialog)(window, cx));
    let mut spec = dialog.into_modal_spec();
    // The layer mask is left transparent (mask: None, not click-closable): the
    // scrim below carries the theme scrim color + per-corner CSD rounding (the
    // LayerStack's plain full-window mask would paint square corners into the
    // transparent CSD surface). The mask div still occludes the page and
    // stops stray clicks; the scrim handles outside-click dismissal.
    spec.mask = None;
    spec.mask_closable = false;
    // Scrim + centered panel: the scrim covers the whole window. Its corners
    // follow the CSD window radius ONLY when the compositor forced Client
    // decorations — under Server (the default on KDE/macOS/Windows) the system
    // frame owns the outline, so the scrim must paint square corners flush
    // into it, never a second app-drawn arc over the native frame. Dismisses
    // on any left click and runs the caller's on_close (cancel protocol). The
    // panel is centered over it; the dialog's own panel stops mouse-down
    // bubbling, so clicks inside the dialog never dismiss through the scrim.
    let scrim_factor = if matches!(window.window_decorations(), gpui::Decorations::Server) {
        0.0
    } else {
        1.0
    };
    let scrim_corners = [
        t.window_corner_radius(WindowCorner::TopLeft) * scrim_factor,
        t.window_corner_radius(WindowCorner::TopRight) * scrim_factor,
        t.window_corner_radius(WindowCorner::BottomRight) * scrim_factor,
        t.window_corner_radius(WindowCorner::BottomLeft) * scrim_factor,
    ];
    let scrim_color = t.scrim;
    let on_close_for_scrim = on_close_for_scrim.clone();
    let inner = spec.content.clone();
    spec.content = Rc::new(
        move |backfill: LayerBackfill, window: &mut Window, cx: &mut App| {
            let close = backfill.close.clone();
            let on_close = on_close_for_scrim.clone();
            let scrim = div()
                .id("modal-scrim")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded_tl(px(scrim_corners[0]))
                .rounded_tr(px(scrim_corners[1]))
                .rounded_br(px(scrim_corners[2]))
                .rounded_bl(px(scrim_corners[3]))
                .bg(taskmanager_ui::theme_binding::fill(scrim_color))
                .on_any_mouse_down(move |event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if event.button == MouseButton::Left {
                        on_close(window, cx);
                        close(window, cx);
                    }
                });
            div()
                .size_full()
                // The scrim and the panel animate INDEPENDENTLY: the scrim
                // fades in fast (DURATION_FAST), the panel fades + rises over
                // the standard appear duration (180ms) — the Mission Center
                // modal entrance. The panel's `pt` is an approximation of an
                // upward slide (no transform in gpui 0.2.2): the centered
                // panel starts ~7px low and rises into place. The ids are
                // stable per dialog host (each host lives in its own subtree —
                // global element ids include the ancestor path), and the
                // state is dropped once the modal closes, so every open
                // replays the entrance.
                .child(
                    scrim.with_animation("modal-scrim-fade", fade_in(), |el, delta| {
                        el.opacity(delta)
                    }),
                )
                .child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(inner(backfill, window, cx))
                        .with_animation("modal-fade", appear(), |el, delta| {
                            el.opacity(delta).pt(px(14.0 * (1.0 - delta)))
                        }),
                )
                .into_any_element()
        },
    );
    stack.update(cx, |stack, cx| {
        if stack.is_empty() {
            stack.push_modal(spec, window, cx);
        }
    });
    stack
}

// ── tooltip ──────────────────────────────────────────────────────────────
// gpui 0.2.2 ships no tooltip. We build one from the same primitives the other
// overlays use — `deferred` (raise z-order above siblings) + `anchored` (absolute
// window-space placement). Unlike [`dialog_overlay`], a tooltip is *non-interactive*:
// it carries no full-size scrim, so it never intercepts pointer events over the host
// it describes (the host stays hoverable). Only the tiny label rect participates in
// hit-testing, which is what every OS tooltip does.

/// The tooltip label visual on its own: a small rounded, shadowed box in the
/// theme's panel surface (`card_surface()` / `fg` / `border` — the same tokens
/// [`tooltip`] itself uses, so popovers read as one family across all 8 skin
/// variants, high-contrast included).
///
/// This is the **stateless leaf**: it knows nothing about hover or position. Use
/// [`tooltip_overlay`] to place it, or compose it directly inside your own
/// `anchored` / `deferred` wrapper if you need custom placement.
pub fn tooltip(t: &Theme, text: &str) -> Div {
    div()
        .bg(taskmanager_ui::theme_binding::fill(t.card_surface()))
        .border_1()
        .border_color(taskmanager_ui::theme_binding::hsla(t.border))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::control_radius(t),
        ))
        .shadow_lg()
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
        .text_color(taskmanager_ui::theme_binding::hsla(t.fg))
        .child(text.to_string())
}

/// A tooltip overlay: the [`tooltip`] label, raised above all siblings via
/// [`deferred`] and placed at window-space point `anchor` via [`anchored`] (whose
/// `position` + `snap_to_window` also flip/snap the label to keep it on-screen
/// when `anchor` nears a window edge).
///
/// `anchor` is interpreted in **window coordinates** (the same space as
/// [`MouseDownEvent::position`] / MouseMoveEvent::position); the label's
/// top-left is placed there. Pass the pointer position (for a cursor-following
/// tooltip) or a host element's bottom-left + a small gap (for a host-anchored
/// tooltip).
///
/// Assemble the tooltip at the **root of `render`**, as a sibling of the body —
/// exactly where [`dialog_overlay`] is layered. The owning view decides whether
/// it is shown:
///
/// ```ignore
/// // in the view:
/// if let Some((pos, txt)) = &self.tooltip {
///     root = root.child(elements::tooltip_overlay(&t, txt, *pos));
/// }
/// ```
///
/// # Attaching it to a host (hover tracking)
/// The primitive deliberately holds no state — the owning view is the source of
/// truth, mirroring how `Pill` / `Switch` all work. Wire the host element
/// with an `on_hover` listener — the same hover-slot pattern the tab bar,
/// sidebar, and chrome already use to track the hovered element:
///
/// ```ignore
/// div()
///     .id("my-host")
///     .on_hover(cx.listener(move |v, is_hov: &bool, _win, cx| {
///         v.set_tooltip(*is_hov, cx);   // toggle view state → cx.notify()
///     }))
///     // optional: anchor the tooltip at the cursor while hovering
///     .on_mouse_move(cx.listener(move |v, ev: &MouseMoveEvent, _win, cx| {
///         if v.tooltip.is_some() {
///             v.tooltip_pos = Some(ev.position);
///             cx.notify();
///         }
///     }))
/// ```
///
/// `set_tooltip` clears `tooltip` on hover-out; the overlay then stops rendering
/// next frame. One slot (e.g. `Option<(Point<Pixels>, String)>` on the view) is
/// enough for the whole window — only one element is hovered at a time.
///
/// **Why no scrim?** [`dialog_overlay`] wraps its content in a `size_full`
/// click-catcher because it MUST dismiss on outside clicks. A tooltip must do
/// the opposite — stay transparent to the pointer so
/// the host remains interactive — so [`tooltip_overlay`] omits the catcher and
/// lets `anchored` place the bare label directly.
pub fn tooltip_overlay(t: &Theme, text: &str, anchor: Point<Pixels>) -> impl IntoElement {
    // Appear transition: the label fades in over DURATION_FAST the first time
    // this text shows. The animation element is keyed by the label so a
    // cursor moving within the same hover target (same text) does not restart
    // it; once the tooltip disappears the element state is dropped and the
    // next appearance fades again (gpui drops element state for unmounted
    // ids — see window.rs finish()).
    let label =
        tooltip(t, text).with_animation("tooltip-fade", fade_in(), |el, delta| el.opacity(delta));
    deferred(anchored().position(anchor).snap_to_window().child(label))
}

// ── elastic text wrapper (gpui "min_w(0) + truncate" pattern) ──────────────
// In a resizable window, a long label inside a flex row overflows, pushes its
// siblings, or clips ugly unless BOTH invariants hold on its flex slot:
//   * `.min_w(px(0.0))` — overrides flex's default `min-width: auto` (which is
//     the content's intrinsic width), so the item is allowed to shrink BELOW its
//     content width when the row runs out of room. Without this the label acts
//     as a rigid spacer and shoves its siblings (gear, window controls, the next
//     tab) off-screen.
//   * `.truncate()` — sets overflow:hidden + text-overflow:ellipsis + white-
//     space:nowrap, so an overflowed label ends in a clean "…" instead of
//     clipping mid-glyph. (Pure overflow:hidden without truncate would just
//     hard-clip; truncate adds the ellipsis and the no-wrap policy.)
// Pair with a hover tooltip on the host (see tooltip_overlay) so the truncated
// tail is recoverable on hover — the chrome tabs and sidebar device rows wire
// this via the `Hover::Static(label)` / `Hover::Device(dev)` hover slot, which
// root.rs resolves through static_label() / device_label() and renders at the
// cursor via tooltip_overlay.

/// The canonical "gpui elastic" wrapper for a text label that lives as a flex
/// child inside a row. [`Div::min_w`] `= 0` lets the flex item shrink below its
/// content's natural width; [`Div::truncate`] adds overflow-hidden + ellipsis +
/// no-wrap. Use it for page tabs, chrome controls, sidebar headings/captions,
/// and dialog header rows — any flex-row label that can overflow when the window
/// narrows.
///
/// Text styling (`text_size`, `text_color`, `font_weight`) is NOT applied here —
/// either chain it on the returned [`Div`] (`.text_size(..)` / `.text_color(..)`
/// / `.font_weight(..)`) or let it inherit from the parent. Returns a plain
/// [`Div`] (not `impl IntoElement`) so the caller can keep chaining layout such
/// as `.flex_1()` (needed when the wrapper is itself a flex item that should
/// grow to fill slack, e.g. the sidebar caption stack).
///
/// ```ignore
/// // page tab label (inherits size/weight/color from the tab div):
/// div()
///     .flex()
///     .child(taskmanager_ui::icons_binding::icon(icon).size(px(14.0)))
///     .child(elements::truncated_text(label));
///
/// // sidebar caption (own styling + flex_1 to fill the caption column):
/// elements::truncated_text(&cap1)
///     .flex_1()
///     .text_size(tokens::FONT_11)
///     .text_color(theme.fg_dim)
/// ```
pub fn truncated_text(text: &str) -> Div {
    div().min_w(px(0.0)).truncate().child(text.to_string())
}

/// The explicit "… {count} more" hint under a list whose widget materialization
/// is bounded while the data stays complete (the container's header keeps the
/// true total). Lists use this instead of silently dropping rows, so a capped
/// surface always tells the user how much exists beyond the cap.
pub fn more_rows_hint(t: &Theme, hidden: usize) -> Div {
    div()
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
        .text_color(taskmanager_ui::theme_binding::hsla(t.fg_dim))
        .child(more_rows_label(hidden))
}

/// Locale formatting for [`more_rows_hint`], split out so the count
/// substitution is unit-testable without a theme.
pub fn more_rows_label(hidden: usize) -> String {
    i18n::t("common.more_rows").replace("{count}", &hidden.to_string())
}
