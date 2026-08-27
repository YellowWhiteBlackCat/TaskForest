//! App-adaptation helpers over the owned component layer (ADR-017). This module
//! is NOT a component library: every reusable primitive lives in
//! `taskmanager-ui` (primitives/inputs/overlays/data), and this file only hosts
//! the business-flavored compositions the app views share — the focus-ring /
//! pill / tool_btn / dialog / graph_card / status_bar family, all parameterized
//! by a `&Theme` and caller-owned state via closures. New primitives must land
//! in `taskmanager-ui`, never here; the UI component architecture record
//! (`docs/UI_COMPONENT_ARCHITECTURE.md` §2.2 "业务组件是资产") treats these as
//! app assets, not generic controls.
//!
//! Overlay/dismiss pattern: the full-size content container carries the
//! `on_mouse_down` close handler; the inner panel calls `cx.stop_propagation()` so
//! clicks on it don't bubble up and dismiss. (Mirrors gpui's own `window/prompts.rs`.)

use gpui::{
    AnimationExt, AnyElement, App, AppContext, BoxShadow, Div, ElementId, Entity, HighlightStyle,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Pixels, Point,
    StatefulInteractiveElement, StyleRefinement, Styled, StyledText, Window, anchored, canvas,
    deferred, div, px,
};
use std::cell::RefCell;
use std::rc::Rc;

use taskmanager_theme::color::mix;
use taskmanager_ui::overlays::dialog::Dialog;
use taskmanager_ui::overlays::layer_stack::{LayerBackfill, LayerStack};
use taskmanager_ui::primitives::motion::{hover_animation, hover_state_key};
use taskmanager_ui::primitives::pill::PillState;
use taskmanager_ui_contract::IconId;

use crate::gpui_app::graph::{GraphSampleState, graph_sample_state};
use crate::gpui_app::icons;
use crate::gpui_app::theme::tokens;
use crate::gpui_app::theme::{Theme, WindowCorner, appear, fade_in};
use crate::i18n;

/// A pill / segmented-control segment. Active = accent fill + white text; inactive
/// picks up a translucent accent overlay on hover. `on_hover` is wired by the caller
/// (via `cx.listener`) so this primitive stays decoupled from any particular view.
///
/// WCAG 2.4.7 (Focus Visible): the pill's own shell is a keyboard tab-stop and
/// draws the 2px accent focus ring via [`focus_ring`] below — guaranteed-visible
/// across all 8 skin variants regardless of pointer modality. Mirrors the
/// `.focus(elements::focus_ring(t))` pattern used in root/chrome.rs.
pub fn pill(
    t: &Theme,
    id: impl Into<ElementId>,
    label: &str,
    active: bool,
    hovered: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Pill::new(id, label, on_click, on_hover)
        .active(active)
        .hovered(hovered)
        .render(t)
}

type ActionHandler = dyn Fn(&mut Window, &mut App);
type HoverHandler = dyn Fn(&bool, &mut Window, &mut App);

/// Builder-style pill: config is field + defaults instead of positional
/// arguments. `new` supplies the two required closures; everything else
/// (`active`, the optional leading icon) has a default and is set via chained
/// setters. Call sites end with `.render(t)` to produce the element.
///
/// The rendered tree is identical to the old `pill` / `pill_with_icon` /
/// `pill_with_semantic_icon` functions: gpui-component `Button` renders
/// `[icon][label]` in an internal h_flex when an icon is set (button.rs), and
/// the icon slot is omitted entirely when none was configured, so the no-icon
/// path stays a single-label Button.
pub struct Pill {
    id: ElementId,
    label: String,
    active: bool,
    hovered: bool,
    enabled: bool,
    icon: Option<IconId>,
    on_click: Rc<ActionHandler>,
    on_hover: Rc<HoverHandler>,
}

impl Pill {
    /// Create a pill with its click/hover closures bound. Defaults: inactive,
    /// no leading icon. The `hovered` bool that legacy `pill` callers passed is
    /// deliberately absent — hover visuals are Button-native and the flag never
    /// influenced rendering.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<String>,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
        on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            active: false,
            hovered: false,
            enabled: true,
            icon: None,
            on_click: Rc::new(on_click),
            on_hover: Rc::new(on_hover),
        }
    }

    /// Active segment → primary fill; inactive → ghost.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Hovered idle segment → translucent accent surface (palette `hover`),
    /// giving idle pills the documented hover affordance they previously lacked.
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// Disable a segment when the provider has no trustworthy history for it.
    /// Disabled pills remain visible as capability evidence, but are removed
    /// from keyboard focus and activation paths.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Leading [`IconId`] glyph beside the label.
    pub fn icon(mut self, icon: IconId) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Leading frontend-neutral semantic icon.
    pub fn semantic_icon(mut self, icon: IconId) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Render the pill with the current theme.
    pub fn render(self, t: &Theme) -> impl IntoElement {
        // taskmanager-ui pill visual (accent fill + on-accent when active,
        // surface + border when idle) inside our own focusable shell. The
        // shell is the keyboard contract the old gc `Button` provided: a real
        // tab stop, the accent focus ring (focus-visible via `Theme`), and
        // Enter/Space activation through gpui's `ClickEvent::Keyboard` for
        // focused elements with click listeners. The `hovered` flag the legacy
        // callers passed never influenced rendering — the shell's hover visual
        // is native, exactly like the gc Button it replaces.
        let on_click = self.on_click;
        let on_hover = self.on_hover;
        let enabled = self.enabled;
        let pill = taskmanager_ui::primitives::pill::Pill::new(
            self.label,
            if self.active {
                PillState::Active
            } else {
                PillState::Idle
            },
            t.palette(),
        )
        .hovered(self.hovered);
        let id = self.id.clone();
        let mut wrapper = div().id(self.id).debug_selector(move || id.to_string());
        if enabled {
            wrapper = wrapper
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .focus(focus_ring(t))
                .on_click(move |_ev, win, cx| on_click(win, cx))
                .on_hover(move |hovered, win, cx| on_hover(hovered, win, cx));
        } else {
            wrapper = wrapper.tab_stop(false).cursor_default().opacity(0.45);
        }
        match self.icon {
            // Same 8px gap the gc Button's internal [icon][label] row used and
            // the same 12px glyph; the icon inherits the page text color, as
            // before (the old icon sibling did the same).
            Some(icon) => wrapper
                .flex()
                .items_center()
                .gap(tokens::SPACE_8)
                .child(icons::icon(icon).size(px(12.0)))
                .child(pill),
            None => wrapper.child(pill),
        }
    }
}

// ── action button + graph card ─────────────────────────────────────────────
// Two shared layout primitives that collapse duplicated inline code across the
// views. `tool_btn` unifies the processes_view `action_btn` / services_view
// `svc_btn` (one inert-button look with an `enabled` flag); `graph_card` wraps
// the recurring rounded/bordered graph container (perf_views + cpu_view).

/// A unified action button (End task / Kill / Start / Restart / ...). Mirrors
/// [`pill`]'s closure shape exactly — `on_click: Fn(&mut Window, &mut App)` /
/// `on_hover: Fn(&bool, &mut Window, &mut App)` — so call sites bind their
/// `Entity<RootView>` updates the same way pills do, and the primitive stays
/// decoupled from `root` (no `Context<RootView>` parameter, which would create a
/// circular `elements` ↔ `root` import). Adds button styling + an `enabled` flag.
///
/// Styling: `px(12)` x / `px(6)` y padding, `rounded(tokens::control_radius(theme))`,
/// `text_size px(13)`, `text_color theme.fg`. When `enabled`: bg = a translucent
/// accent overlay when `hovered`, else `theme.sidebar_card_bg`; `cursor_pointer`;
/// `on_click` + `on_hover` wired. When `!enabled`: opacity 0.4, `theme.fg_dim`
/// text, `theme.sidebar_card_bg` fill, `cursor_default`, and NO `on_click` /
/// `on_hover` wired (inert) — matches the look of the old `action_btn` / `svc_btn`.
/// The caller owns the `hovered` bool and publishes its hover slot via `on_hover`,
/// exactly as pills do.
pub fn tool_btn(
    theme: &Theme,
    id: impl Into<ElementId>,
    label: &str,
    enabled: bool,
    hovered: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    ToolBtn::new(id, label, on_click, on_hover)
        .enabled(enabled)
        .hovered(hovered)
        .render(theme)
}

/// Builder-style toolbar button: `new` binds the two closures; `enabled`,
/// `hovered` and the optional leading icon are defaulted fields set via chained
/// setters, rendered with `.render(theme)`. Replaces the 8-argument
/// `tool_btn_with_icon` / `tool_btn_with_semantic_icon` functions with the same
/// rendered tree: the label slot becomes a flex row (`[icon][label]`) only when
/// an icon is passed; the no-icon arm stays a plain Block div with one text
/// child (gpui `Style::default()` is `Display::Block`), so existing callers see
/// zero layout change.
pub struct ToolBtn {
    id: ElementId,
    label: String,
    enabled: bool,
    hovered: bool,
    icon: Option<IconId>,
    on_click: Rc<ActionHandler>,
    on_hover: Rc<HoverHandler>,
}

impl ToolBtn {
    /// Create a tool button with its closures bound. Defaults: enabled,
    /// not hovered, no leading icon.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<String>,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
        on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
            hovered: false,
            icon: None,
            on_click: Rc::new(on_click),
            on_hover: Rc::new(on_hover),
        }
    }

    /// Enabled buttons are clickable, keyboard-focusable and hover-highlighted;
    /// disabled ones render inert (dimmed, default cursor, no handlers).
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Hover state, owned by the caller and published via `on_hover`, drives
    /// the translucent accent overlay while enabled.
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// Leading [`IconId`] glyph beside the label.
    pub fn icon(mut self, icon: IconId) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Leading frontend-neutral semantic icon.
    pub fn semantic_icon(mut self, icon: IconId) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Render the tool button with the current theme.
    pub fn render(self, theme: &Theme) -> impl IntoElement {
        // Disabled: dim opacity, dim text (theme.fg_dim), default cursor, surface fill,
        // no hover overlay, and no on_click/on_hover wired (inert) — matches
        // action_btn (processes_view) / svc_btn (services_view).
        let fg = if self.enabled { theme.fg } else { theme.fg_dim };
        // `.id(id)` makes the div Stateful so on_click/on_hover can attach; px/py/bg/
        // text styling + .child() all preserve Stateful<Div>, so both branches below
        // assign the same type back to `btn`.
        //
        // WCAG 2.4.7 (Focus Visible): `.focusable()` creates the focus handle,
        // `.tab_stop(true)` enters it into keyboard order, and
        // `.focus(focus_ring(theme))` paints the 2px accent outset ring while focused —
        // the same pattern root/chrome.rs uses on tabs + the gear button. Applied to
        // the base Stateful<Div> so BOTH the enabled (clickable) and disabled (inert)
        // branches below inherit the ring + tab-stop.
        let btn = div()
            .id(self.id)
            .px(tokens::SPACE_12)
            .py(tokens::SPACE_6)
            .rounded(tokens::control_radius(theme))
            .text_size(tokens::FONT_14)
            .text_color(fg);
        let mut btn = if self.enabled {
            btn.focusable().tab_stop(true).focus(focus_ring(theme))
        } else {
            btn
        };
        // The hover background is a keyed 120ms transition painted by an
        // absolute overlay UNDER the label (idle → sidebar_card_bg, hovered →
        // translucent accent tint eased in). The overlay stays a descendant
        // of the focusable shell — a keyed animation id that changes between
        // frames on a focused element's ancestor path breaks gpui 0.2.2 key
        // dispatch, so the animation never wraps the shell.
        if self.enabled {
            let base = theme.sidebar_card_bg;
            let hover = theme.hover_bg();
            let hovered = self.hovered;
            btn = btn.bg(base).relative().child(
                div().absolute().inset_0().child(
                    div()
                        .size_full()
                        .rounded(tokens::control_radius(theme))
                        .with_animation(
                            ("tool-btn-bg", hover_state_key(false, hovered)),
                            hover_animation(),
                            move |el, delta| {
                                if hovered {
                                    el.bg(mix(base, hover, delta))
                                } else {
                                    el.bg(base)
                                }
                            },
                        ),
                ),
            );
        } else {
            btn = btn.bg(theme.sidebar_card_bg);
        }
        // Label slot. The icon inherits `fg` from this div (the same text_color
        // inheritance chrome tabs rely on for their Icon child).
        btn = match self.icon {
            Some(ic) => btn
                .flex()
                .items_center()
                .gap(tokens::SPACE_6)
                .child(icons::icon(ic).size(px(13.0)))
                .child(self.label),
            None => btn.child(self.label),
        };
        if self.enabled {
            // Match pill's wrapper: discard the event arg, forward (win, cx).
            let on_click = self.on_click;
            let on_hover = self.on_hover;
            btn = btn
                .cursor_pointer()
                .on_click(move |_ev, win, cx| on_click(win, cx))
                .on_hover(move |hovered, win, cx| on_hover(hovered, win, cx));
        } else {
            btn = btn.cursor_default().opacity(0.4);
        }
        btn
    }
}

/// The card shadow's ambient layer: the share of the ink alpha, the drop, and
/// the blur radius. 2026-08 稳固效果 policy (owner: "很深的 blur 效果很糟糕")
/// — the pre-change y4/blur16 halo read as heavy blur on EVERY graph card at
/// once and muddied the gaps between adjacent cards; separation now comes
/// from the tone ladder (window → sidebar → card fills + 1px border) and the
/// shadow is only a whisper of lift. Shared with the shadow test so the
/// contract lives in one place.
pub(crate) const CARD_SHADOW_AMBIENT_ALPHA: f32 = 0.35;
pub(crate) const CARD_SHADOW_AMBIENT_DROP: f32 = 2.0;
pub(crate) const CARD_SHADOW_AMBIENT_BLUR: f32 = 6.0;

/// The soft two-layer card shadow: a low, low-opacity ambient blur plus a
/// tight, high-opacity edge blur — both painted in the theme's single
/// `card_shadow` ink (the edge layer carries the full ink alpha, the ambient
/// layer a fixed share of it — see the `CARD_SHADOW_AMBIENT_*` constants).
/// Cards and dashboard tiles read
/// through this helper. Dense list rows (the 10k-process table) NEVER do —
/// per-row shadows would add a shadow pass per row on every scroll frame
/// (performance discipline, see tokens.rs motion policy).
pub fn card_shadow(t: &Theme) -> Vec<BoxShadow> {
    let ink = t.card_shadow();
    vec![
        BoxShadow {
            color: ink.with_alpha(ink.a * CARD_SHADOW_AMBIENT_ALPHA).into(),
            offset: Point::new(px(0.0), px(CARD_SHADOW_AMBIENT_DROP)),
            blur_radius: px(CARD_SHADOW_AMBIENT_BLUR),
            spread_radius: px(0.0),
        },
        BoxShadow {
            color: ink.into(),
            offset: Point::new(px(0.0), px(1.0)),
            blur_radius: px(4.0),
            spread_radius: px(0.0),
        },
    ]
}

/// The recurring graph-container wrapper used by every Performance graph card
/// (perf_views.rs `render_memory` + `main_with_stats`, and cpu_view.rs per-core
/// grid plus headline). Pure layout helper — a flex-filling, rounded, 1px-bordered
/// card surfaced in the theme's elevated card fill (`Theme::card_surface`) that
/// clips its graph to the rounded corners via `overflow_hidden`. Carries the
/// two-layer [`card_shadow`]. Collapses the
/// 4 inline copies into one call.
///
/// Returns a `Div` (not `impl IntoElement`) so callers can keep chaining layout
/// (e.g. an absolute overlay label on top of the graph, as cpu_view per-core does).
pub fn graph_card(theme: &Theme, graph: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_h(px(0.0))
        .rounded(tokens::card_radius(theme))
        .border(px(1.0))
        .border_color(theme.border)
        .bg(theme.card_surface())
        .shadow(card_shadow(theme))
        .overflow_hidden()
        .child(graph)
}

/// Graph card with an honest first-frame status overlay.
///
/// A blank canvas is ambiguous when a provider has not published its first
/// sample yet or has explicitly reported a gap for every slot. The overlay is
/// intentionally a small centered label: it preserves the grid/card geometry,
/// leaves the graph's color identity intact, and makes the state readable at
/// both wide and compact sizes without fabricating a zero trace.
pub fn graph_card_with_state(theme: &Theme, graph: impl IntoElement, samples: &[f32]) -> Div {
    graph_card_with_explicit_state(theme, graph, graph_sample_state(samples))
}

/// The two-series variant of [`graph_card_with_state`]: the first-frame state
/// is classified over the UNION of the two directions' evidence (see
/// `graph::graph_dual_sample_state`), so a measured read direction is not
/// mislabeled unavailable when the summed lane or the write direction holds
/// only gaps.
pub fn graph_card_with_dual_state(
    theme: &Theme,
    graph: impl IntoElement,
    primary: &[f32],
    secondary: &[f32],
) -> Div {
    graph_card_with_explicit_state(
        theme,
        graph,
        crate::gpui_app::graph::graph_dual_sample_state(primary, secondary),
    )
}

fn graph_card_with_explicit_state(
    theme: &Theme,
    graph: impl IntoElement,
    state: GraphSampleState,
) -> Div {
    let mut host = div().relative().size_full().child(graph);
    if state != GraphSampleState::Measured {
        let label = match state {
            GraphSampleState::Collecting => i18n::t("common.collecting_telemetry"),
            GraphSampleState::Unavailable => i18n::t("dashboard.unavailable"),
            GraphSampleState::Measured => "",
        };
        host = host.child(
            div()
                .id("tm-graph-state")
                .debug_selector(|| "tm-graph-state".to_string())
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .px(tokens::SPACE_12)
                        .py(tokens::SPACE_6)
                        .rounded(tokens::control_radius(theme))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.card_surface())
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg_dim)
                        .child(label),
                ),
        );
    }
    graph_card(theme, host)
}

/// One entry of a graph legend: the series' stroke color and its localized
/// direction label ("Read"/"Write", "Receive"/"Send").
pub struct GraphLegendEntry {
    pub color: gpui::Rgba,
    pub label: String,
}

/// The mini legend above a two-series graph card: one color swatch + label
/// per series, in the caller's order (primary first). This is the GPUI
/// component-language rendering of the semantic iced draws into its canvas
/// (`device_chart::multi::draw_chart_legend`) — swatch quads and shaped text
/// come from the div/text system, not the paint closure, so legend relabels
/// never touch the tessellated graph scene.
pub fn graph_legend(theme: &Theme, entries: &[GraphLegendEntry]) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .gap(tokens::SPACE_12)
        .w_full()
        .min_w(px(0.0))
        .text_size(tokens::FONT_11)
        .debug_selector(|| "tm-graph-legend".to_string());
    for (index, entry) in entries.iter().enumerate() {
        row = row.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(tokens::SPACE_4)
                .child(
                    div()
                        .size(px(8.0))
                        .rounded(px(2.0))
                        .bg(entry.color)
                        .debug_selector(move || format!("tm-graph-legend-swatch:{index}")),
                )
                .child(
                    div()
                        .text_color(theme.fg_dim)
                        .child(entry.label.clone())
                        .debug_selector(move || format!("tm-graph-legend-label:{index}")),
                ),
        );
    }
    row
}

// ── window-activation-aware chrome borders (Zed #2610) ────────────────────
// When the window is inactive, CSD chrome should recede behind the user's
// other windows: the 1px titlebar border drops to 60% alpha so the frame
// reads as system chrome instead of content. This mapping is the single
// source of truth for active→border strength; the render layer feeds it the
// live activation flag (`Window::is_window_active`, via the app's
// active-window handle) every frame.

/// Alpha multiplier applied to the titlebar border while the window is
/// inactive. 0.6 keeps the 1px line visible against the titlebar surface in
/// both modes while clearly weakening it (GNOME dims inactive decorations to
/// ~0.6 too).
const INACTIVE_BORDER_ALPHA: f32 = 0.6;

/// The titlebar border color under the current window activation state.
///
/// Active windows keep the theme's `border` token untouched — screenshot
/// baselines depend on the exact active look. Inactive windows render the
/// same hue at INACTIVE_BORDER_ALPHA alpha: the border tokens are opaque
/// hexes, so alpha scaling dims uniformly across all 8 skin variants without
/// inventing a per-theme dim color.
pub fn titlebar_border(t: &Theme, active: bool) -> gpui::Rgba {
    if active {
        t.border.into()
    } else {
        t.border
            .with_alpha(t.border.a * INACTIVE_BORDER_ALPHA)
            .into()
    }
}

// ── focus ring (WCAG 2.4.7 Focus Visible) ──────────────────────────────────
// gpui 0.2.2 focus API, verified in gpui's src/elements/div.rs + src/window.rs:
//   * `.id(..)` on a `Div` → `Stateful<Div>`, which implements BOTH
//     `InteractiveElement` (the `.focus(|s| ..)` stateful style hook) and
//     `StatefulInteractiveElement` (`.focusable()` + `.tab_stop(true)`).
//   * `.focusable()` creates the persistent per-id `FocusHandle`, but does NOT by
//     itself enter the keyboard order; custom div controls also need
//     `.tab_stop(true)`. A stateless render fn needs no caller-side handle storage.
//   * `.focus(|s| ..)` is the focus analogue of `.hover(|s| ..)`: gpui applies
//     the refinement only when the element's tracked handle is focused
//     (div.rs:2490). `RootView` capture listeners track input modality once per
//     window; its Theme render snapshot lets this shared hook paint only for
//     keyboard focus without per-control state or thread-local storage.
//   * Bonus: a focusable element with `.on_click(..)` is ALSO keyboard-
//     activatable — gpui fires `ClickEvent::Keyboard` on Enter/Space (div.rs:2198),
//     so WCAG 2.1.1 (Keyboard) comes for free with 2.4.7.
//
// `FocusId` still carries no input origin. The application therefore derives
// focus-visible from root capture order: key capture selects Keyboard before Tab
// moves focus; pointer capture selects Pointer before a descendant click focuses.

/// The accent-colored focus ring drawn around a focused chrome control: a solid
/// 2px ring at ~0.6 alpha. Implemented as a 0-blur, 0-offset box-shadow with
/// +2px `spread_radius`, so the ring sits OUTSIDE the element and does NOT
/// perturb layout (unlike `border`, which would shove content inward by 2px the
/// frame focus lands). gpui 0.2.2's `BoxShadow` has no `inset` field, so this is
/// an outset ring — fine for chrome controls, which have titlebar breathing room
/// around them.
///
/// This is the raw [`BoxShadow`] value for callers composing a custom
/// [`StyleRefinement`]. For the common case prefer [`focus_ring`] — a drop-in
/// closure for gpui's `.focus(..)` hook. Both stay in theme via [`Theme::accent`]
/// so the ring adapts to all 8 skin variants.
pub fn focus_ring_shadow(t: &Theme) -> BoxShadow {
    BoxShadow {
        color: t.accent.with_alpha(0.6).into(),
        offset: Point::default(),
        blur_radius: px(0.0),
        spread_radius: px(2.0),
    }
}

/// A drop-in adapter for gpui's `.focus(..)` stateful style hook: pass
/// `elements::focus_ring(t)` straight to `.focus(..)` on any `.focusable()`
/// element and a 2px accent ring appears around it while focused, with no
/// `FocusHandle` plumbing on the call site.
///
/// ```ignore
/// div()
///     .id("my-btn")
///     .focusable()
///     .tab_stop(true)
///     .focus(elements::focus_ring(t))
///     .on_click(..)
/// ```
///
/// See [`focus_ring_shadow`] for the ring geometry and the section header above
/// for the full gpui focus API + the focus-visible finding.
pub fn focus_ring(t: &Theme) -> impl FnOnce(StyleRefinement) -> StyleRefinement {
    let shadow = t.focus_visible().then(|| focus_ring_shadow(t));
    move |s: StyleRefinement| match shadow {
        Some(shadow) => s.shadow(vec![shadow]),
        None => s,
    }
}

/// Bottom status bar (Win11 TM / Mission Center parity): a border-top strip
/// with left-aligned summary parts and right-aligned readouts.
pub fn status_bar(theme: &Theme, left: &[String], right: &[String]) -> Div {
    div()
        .debug_selector(|| "tm-status-bar".into())
        .h(px(26.0))
        .flex_shrink_0()
        .border_t_1()
        .border_color(theme.border)
        .px(tokens::SPACE_12)
        .flex()
        .items_center()
        .gap(tokens::SPACE_16)
        .text_size(tokens::FONT_11)
        .text_color(theme.fg_dim)
        .child(
            div()
                .flex()
                .items_center()
                .gap(tokens::SPACE_16)
                .children(left.iter().map(|part| div().child(part.clone()))),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .gap(tokens::SPACE_16)
                .children(right.iter().map(|part| div().child(part.clone()))),
        )
}

/// Search-match highlighting: `text` with every `query` occurrence painted in
/// the theme's highlight token (`Theme::highlight_fg`, currently the accent;
/// kept as a distinct semantic so search legibility cannot drift from the
/// accent family) — ASCII case-insensitive, non-overlapping, via the SINGLE
/// shared match engine `taskmanager_application::text::match_ranges_ascii_ci`
/// (ADR-020: GPUI, TUI and iced all render these ranges and never recompute
/// matches themselves; the `taskmanager-ui` highlighter component remains for
/// other consumers but is no longer the GPUI search-match source).
/// Falls back to a plain text element when the query is empty.
pub fn highlighted_text(text: &str, query: &str, theme: &Theme) -> impl IntoElement {
    let query = query.trim();
    if query.is_empty() {
        return div().child(text.to_string()).into_any_element();
    }
    let matches = taskmanager_application::text::match_ranges_ascii_ci(text, query);
    highlighted_text_with_ranges(text, &matches, theme)
}

/// Range-driven variant of [`highlighted_text`] for callers that precompute
/// the match ranges once per projection change (the processes table's
/// visible-row projection keys them with the query; see
/// `processes_view::rows::VisibleRow::name_highlights`). A repaint then
/// renders the cached ranges instead of re-running the match engine per row
/// per frame.
pub fn highlighted_text_with_ranges(
    text: &str,
    ranges: &[std::ops::Range<usize>],
    theme: &Theme,
) -> AnyElement {
    // Keep the complete label in ONE text layout.  Building a flex `div` for
    // every match segment makes each segment an independent flex item: when a
    // table column becomes constrained, GPUI can wrap or shrink the segments
    // independently.  That was the source of names appearing one character
    // per line, or only the matching fragment remaining visible.
    //
    // `StyledText` applies the ranges after the parent text style has been
    // resolved, so the non-matching text keeps the cell's inherited color and
    // only the matching bytes receive the search color.  The matcher produces
    // non-overlapping UTF-8 byte ranges, which is exactly the contract expected
    // by `with_highlights`.
    let mut label = StyledText::new(text.to_string());
    if !ranges.is_empty() {
        let highlight = HighlightStyle {
            color: Some(theme.highlight_fg().into()),
            ..Default::default()
        };
        label = label.with_highlights(ranges.iter().cloned().map(|range| (range, highlight)));
    }
    label.into_any_element()
}

/// A minimal sparkline: a thin polyline of `samples` (newest-last), NO axes, NO
/// fill, sized `w`×`h` px and stroked in `color`. Used for the per-row CPU trend in
/// the Processes table (see processes_view::proc_row).
///
/// **Empty `samples`** (cold-start — no history yet) render a flat baseline at the
/// vertical midpoint so the row height stays stable and nothing panics.
/// Non-finite samples are explicit provider gaps: finite runs retain their
/// original time positions and are never connected across the missing slots.
///
/// Vertical scale auto-ranges to the sample set's OWN max (≥ a tiny floor so an
/// all-zero history still draws the baseline) — standard sparkline behavior for a
/// tiny, axis-less mini-chart where two rows aren't meant to be compared in
/// amplitude. Drawn with the same `canvas` + `PathBuilder::stroke` primitives the
/// symbolic icons use (see magnifier_icon); NO fill, NO grid. Deliberately
/// hand-rolled rather than reusing graph_element
/// because that recipe always paints a filled area + grid and scales against a
/// fixed `max`, neither of which fits a 48×16 axis-less mini-chart.
///
/// `samples` accepts a `Vec<f32>`, `Rc<[f32]>`, or `&[f32]` (`Into<Rc<[f32]>>`);
/// the per-row projection passes the shared `Rc` so a repaint pays an `Rc` clone
/// instead of copying the history. Built stroke paths are cached across frames
/// by the sparkline scene store (see `graph::scene_cache`): frames that do not
/// move the canvas or change the data replay the cached paths, and dense runs
/// are LTTB-decimated to the pixel budget so the per-frame rebuilds that
/// horizontal scrolling forces stay cheap.
pub fn sparkline(
    samples: impl Into<Rc<[f32]>>,
    color: gpui::Rgba,
    w: f32,
    h: f32,
) -> impl IntoElement {
    let samples: Rc<[f32]> = samples.into();
    canvas(
        |_bounds, _window, _cx| (),
        move |bounds, _t, window, _cx| {
            for path in
                crate::gpui_app::graph::scene_cache::sparkline_paths(&samples, bounds, color)
            {
                window.paint_path(path, color);
            }
        },
    )
    .w(px(w))
    .h(px(h))
}

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
                .bg(scrim_color)
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
        .bg(t.card_surface())
        .border_1()
        .border_color(t.border)
        .rounded(tokens::control_radius(t))
        .shadow_lg()
        .px(tokens::SPACE_8)
        .py(tokens::SPACE_4)
        .text_size(tokens::FONT_12)
        .text_color(t.fg)
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
/// truth, mirroring how [`pill`] / `switch` all work. Wire the host element
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
///     .child(icons::icon(icon).size(px(14.0)))
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
        .text_size(tokens::FONT_11)
        .text_color(t.fg_dim)
        .child(more_rows_label(hidden))
}

/// Locale formatting for [`more_rows_hint`], split out so the count
/// substitution is unit-testable without a theme.
pub fn more_rows_label(hidden: usize) -> String {
    i18n::t("common.more_rows").replace("{count}", &hidden.to_string())
}

// ── stateful entity factory (Input box wiring) ─────────────────────────────
// The search/run Input boxes scattered across the views each carry a verbatim copy of
// the same wiring: root.rs `init_search_entity` (Apps) + `init_run_entity` (Run dialog),
// services_view.rs `init_search_entity`, startup_view.rs `init_search_entity`. Each one
// (1) builds an `Entity<InputState>` with a placeholder via `cx.new(...)`, and (2)
// subscribes `InputEvent::Change` once to pipe the new value into some view-specific
// sink (`RootView.search_query` / `UiState.query`). Run-command submission reads its
// own input entity directly and keeps no parallel string mirror. The old shared
// `make_search_entity` factory was removed once every view adopted its own copy.

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/elements/tests.rs"]
mod tests;
