//! Business-flavored action and pill elements.

use super::focus_ring;
use gpui::{
    AnimationExt, App, ElementId, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use std::rc::Rc;
use taskmanager_theme::Theme;
use taskmanager_theme::color::mix;
use taskmanager_theme::tokens;
use taskmanager_ui::primitives::motion::{hover_animation, hover_state_key};
use taskmanager_ui::primitives::pill::PillState;
use taskmanager_ui_contract::IconId;

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
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .child(taskmanager_ui::icons_binding::icon(icon).size(px(12.0)))
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
            .px(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_12,
            ))
            .py(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_6,
            ))
            .rounded(taskmanager_ui::theme_binding::absolute(
                tokens::control_radius(theme),
            ))
            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_14))
            .text_color(taskmanager_ui::theme_binding::hsla(fg));
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
            btn = btn
                .bg(taskmanager_ui::theme_binding::fill(base))
                .relative()
                .child(
                    div().absolute().inset_0().child(
                        div()
                            .size_full()
                            .rounded(taskmanager_ui::theme_binding::absolute(
                                tokens::control_radius(theme),
                            ))
                            .with_animation(
                                ("tool-btn-bg", hover_state_key(false, hovered)),
                                hover_animation(),
                                move |el, delta| {
                                    if hovered {
                                        el.bg(taskmanager_ui::theme_binding::fill(mix(
                                            base, hover, delta,
                                        )))
                                    } else {
                                        el.bg(taskmanager_ui::theme_binding::fill(base))
                                    }
                                },
                            ),
                    ),
                );
        } else {
            btn = btn.bg(taskmanager_ui::theme_binding::fill(theme.sidebar_card_bg));
        }
        // Label slot. The icon inherits `fg` from this div (the same text_color
        // inheritance chrome tabs rely on for their Icon child).
        btn = match self.icon {
            Some(ic) => btn
                .flex()
                .items_center()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
                .child(taskmanager_ui::icons_binding::icon(ic).size(px(13.0)))
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
