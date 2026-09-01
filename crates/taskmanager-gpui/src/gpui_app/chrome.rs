//! Client-side window chrome (CSD titlebar). The window-frame mode follows a
//! user preference (Settings → Appearance → Window frame): `System` requests
//! native decorations and falls back to this module's widgets when the
//! compositor refuses (the historical behavior — macOS/Windows/KDE grant the
//! native frame, so these widgets stay unused there; GNOME/Mutter and tiling
//! WMs force Client (CSD) and we render native-looking controls here, branched
//! on [`Theme::window_controls`]):
//!   macOS   → three traffic-light circles, top-LEFT (close/min/zoom)
//!   Windows → caption buttons, top-RIGHT (min/max/close)
//!   KDE     → min/max/close, top-RIGHT
//!   GNOME   → single circular Close, top-RIGHT (Adwaita default)
//! `Custom` requests client decorations up front so this module renders even
//! where a native frame is available (the Zed-style rounded app chrome).
//! Rendering ALWAYS follows the granted `window.window_decorations()` fact —
//! the preference only drives the request — so a refused request degrades to
//! whichever audited mode was actually granted, reported honestly (see
//! `decoration_outcome_notice`).
//! Any empty area of the titlebar is a drag handle (`Window::start_window_move`).
//! The render-time gate that suppresses this whole titlebar under Server lives
//! in `root::render` (the `server_decorations` branch).

use crate::gpui_app::elements;
use crate::gpui_app::root::{Hover, RootView};
use gpui::{
    App, Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement, Rgba,
    StatefulInteractiveElement, Styled, Window, div, px, rgb,
};
use taskmanager_core::core::config::{
    WINDOW_DECORATIONS_CUSTOM, WINDOW_DECORATIONS_NATIVE, WINDOW_DECORATIONS_SYSTEM,
};
use taskmanager_theme::WindowControls;
use taskmanager_theme::tokens;
use taskmanager_theme::{Color, Skin, Theme};

/// User-selected window-frame policy (Settings → Appearance → Window frame).
///
/// `System` (the default) keeps the long-standing negotiation: request native
/// decorations, then follow what the window system actually grants. `Native`
/// makes the same request explicitly and expects it to be honored — a
/// compositor that cannot draw server-side decorations (GNOME/Mutter) will
/// refuse, and the refusal is reported to the user. `Custom` requests client
/// decorations up front so the app draws its own titlebar with transparent
/// rounded corners even where a native frame is available.
///
/// The preference only drives the REQUEST (`requested_decorations`, applied at
/// window creation and re-requestable live via
/// `Window::request_decorations`); rendering always follows the granted
/// `window.window_decorations()` fact, so no mixed titlebar can ever render.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WindowDecorationsPreference {
    /// Follow the compositor negotiation; never reports an outcome notice.
    #[default]
    System,
    /// Prefer the OS-drawn titlebar (KDE/KWin, macOS, Windows).
    Native,
    /// Prefer the app-drawn titlebar with transparent rounded corners.
    Custom,
}

impl WindowDecorationsPreference {
    /// Parse a persisted [`taskmanager_core::core::config::Config::window_decorations`]
    /// token. Empty and unknown values fail closed to [`Self::System`] (the
    /// negotiation default) so a hand-edited or newer config can never force a
    /// frame this build cannot deliver.
    #[must_use]
    pub fn from_config_token(token: &str) -> Self {
        match token.trim() {
            WINDOW_DECORATIONS_NATIVE => Self::Native,
            WINDOW_DECORATIONS_CUSTOM => Self::Custom,
            _ => Self::System,
        }
    }

    /// The persisted token for this preference. System persists as the empty
    /// sentinel so an unchanged preference round-trips byte-identically with
    /// pre-preference config files.
    #[must_use]
    pub const fn config_token(self) -> &'static str {
        match self {
            Self::System => WINDOW_DECORATIONS_SYSTEM,
            Self::Native => WINDOW_DECORATIONS_NATIVE,
            Self::Custom => WINDOW_DECORATIONS_CUSTOM,
        }
    }

    /// The decoration mode to request from the window system. `System` and
    /// `Native` both request the native frame (System silently accepts the CSD
    /// fallback when refused; Native expects it to be honored); `Custom`
    /// requests client decorations.
    #[must_use]
    pub const fn requested_decorations(self) -> gpui::WindowDecorations {
        match self {
            Self::System | Self::Native => gpui::WindowDecorations::Server,
            Self::Custom => gpui::WindowDecorations::Client,
        }
    }
}

/// Why an applied window-frame preference was refused by the window system.
/// Emitted at most once per preference change, after the compositor's
/// authoritative configure contradicts the optimistic request echo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecorationOutcomeNotice {
    /// Native was requested but the compositor granted client decorations
    /// (GNOME/Mutter never draws server-side frames). The audited CSD
    /// fallback with rounded corners remains fully usable.
    NativeRefused,
    /// Client was requested but the compositor configured the native frame
    /// anyway. The native titlebar remains fully usable.
    CustomRefused,
}

impl DecorationOutcomeNotice {
    /// The i18n key of the honest user-facing notice for this outcome.
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::NativeRefused => "notice.deco_native_refused",
            Self::CustomRefused => "notice.deco_custom_refused",
        }
    }
}

/// Compare a window-frame preference against the decoration fact the window
/// system actually granted. `System` never produces a notice (it never
/// promised a mode); an explicit preference contradicted by the granted fact
/// does, so the user learns their choice could not be applied.
///
/// This is only ever called with the post-negotiation fact. gpui's request
/// echo is optimistic (the reported mode equals the request until the
/// compositor's configure arrives), so an observed contradiction IS the
/// compositor's verdict, never a transient race artifact.
#[must_use]
pub const fn decoration_outcome_notice(
    pref: WindowDecorationsPreference,
    granted_server: bool,
) -> Option<DecorationOutcomeNotice> {
    match pref {
        WindowDecorationsPreference::System => None,
        WindowDecorationsPreference::Native if !granted_server => {
            Some(DecorationOutcomeNotice::NativeRefused)
        }
        WindowDecorationsPreference::Custom if granted_server => {
            Some(DecorationOutcomeNotice::CustomRefused)
        }
        _ => None,
    }
}

/// Per-skin titlebar height (macOS tight, GNOME tall headerbar, KDE/Win medium).
pub fn titlebar_height(t: &Theme) -> f32 {
    match t.skin {
        Skin::Macos => 34.0,
        Skin::Gnome => 46.0,
        Skin::Kde => 38.0,
        Skin::Windows => 36.0,
    }
}

/// A flex-1 spacer that starts a window-move (drag) on left mouse-down.
pub fn drag_region(id: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .flex_1()
        .h_full()
        .on_mouse_down(MouseButton::Left, |_ev, window, _cx| {
            window.start_window_move();
        })
}

/// macOS traffic-light buttons (close / minimize / zoom), drawn top-left. On hover
/// each light reveals its symbol (× / − / ▢), matching macOS behavior.
pub fn traffic_lights(
    t: &Theme,
    hovered: Option<&Hover>,
    tray_available: bool,
    cx: &mut Context<RootView>,
) -> Div {
    let close_action = if tray_available {
        minimize_to_tray
    } else {
        close_and_quit
    };
    div()
        .h_full()
        .pl(px(13.0))
        .pr(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .flex()
        .flex_row()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(light(
            t,
            rgb(0xff5f57),
            "tl-close",
            "\u{2715}",
            hovered,
            close_action,
            cx,
        ))
        .child(light(
            t,
            rgb(0xfebc2e),
            "tl-min",
            "\u{2013}",
            hovered,
            minimize_window,
            cx,
        ))
        .child(light(
            t,
            rgb(0x28c840),
            "tl-zoom",
            "\u{25a2}",
            hovered,
            zoom_window,
            cx,
        ))
}

fn light(
    t: &Theme,
    color: Rgba,
    id: &'static str,
    glyph: &'static str,
    hovered: Option<&Hover>,
    action: fn(&mut Window, &mut App),
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let is_hov = hovered == Some(&Hover::Static(id));
    // WCAG hit pad: an invisible 28×28 click/hover footprint wraps the 12px visual
    // dot, so the small colored circle is no longer its own (too-small) click target.
    // Hover tracking + click dispatch live on the outer pad; the inner dot is purely
    // visual at its original size/color.
    //
    // WCAG 2.4.7 (Focus Visible): the pad is `.focusable()` +
    // `.tab_stop(true)` + `.focus(focus_ring(t))` so a 2px accent ring draws
    // around it on keyboard focus (and Enter/Space fires `action` — gpui
    // dispatches `ClickEvent::Keyboard` to `.on_click`). See
    // [`elements::focus_ring`] for the gpui focus API + focus-visible notes.
    div()
        .id(id)
        // Zero-cost debug tag (no-op in release) so render tests can confirm the
        // app-drawn window controls render ONLY in the CSD fallback.
        .debug_selector(move || id.to_string())
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(t))
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_hover(cx.listener(move |v, is_hov: &bool, _win, cx| {
            v.set_hover(
                if *is_hov {
                    Some(Hover::Static(id))
                } else {
                    None
                },
                cx,
            );
        }))
        .on_click(move |_ev, window, cx| action(window, cx))
        .child(
            div()
                .size(px(12.0))
                .rounded_full()
                .bg(color)
                .flex()
                .items_center()
                .justify_center()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_8))
                .text_color(rgb(0x000000))
                .child(if is_hov { glyph } else { "" }),
        )
}

/// Right-side window controls (Windows caption / KDE / GNOME close-only).
pub fn window_controls_right(
    t: &Theme,
    hovered: Option<&Hover>,
    tray_available: bool,
    cx: &mut Context<RootView>,
) -> Div {
    let close_action = if tray_available {
        minimize_to_tray
    } else {
        close_and_quit
    };
    let mut row = div().h_full().flex().flex_row().items_center();
    match t.window_controls {
        WindowControls::Breeze | WindowControls::Caption => {
            // Win11 caption ~46×32 flat hit targets; KDE similar. The close button
            // fills red on hover (Win11/Plasma idiom); min/max get a subtle shade overlay.
            row = row
                .child(cap_btn(
                    t,
                    "wnd-min",
                    "\u{2013}",
                    hovered,
                    false,
                    minimize_window,
                    cx,
                ))
                .child(cap_btn(
                    t,
                    "wnd-max",
                    "\u{25a2}",
                    hovered,
                    false,
                    zoom_window,
                    cx,
                ))
                .child(cap_btn(
                    t,
                    "wnd-close",
                    "\u{2715}",
                    hovered,
                    true,
                    close_action,
                    cx,
                ));
        }
        WindowControls::AdwaitaClose => {
            row = row.child(close_circle(t, hovered, close_action, cx));
        }
        WindowControls::TrafficLight => {}
    }
    row
}

/// A flat Windows/KDE caption button with a centered glyph. `is_close` selects the
/// red close-on-hover fill (Win11 #E81123) versus the neutral shade overlay.
fn cap_btn(
    t: &Theme,
    id: &'static str,
    glyph: &'static str,
    hovered: Option<&Hover>,
    is_close: bool,
    action: fn(&mut Window, &mut App),
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let is_hov = hovered == Some(&Hover::Static(id));
    let (bg, fg) = if is_hov && is_close {
        (Color::from_hex(0xe81123), Color::WHITE)
    } else if is_hov {
        (t.shade.with_alpha(0.12), t.fg)
    } else {
        (Color::TRANSPARENT, t.fg)
    };
    div()
        .id(id)
        // Zero-cost debug tag (no-op in release) so render tests can confirm the
        // app-drawn caption controls render ONLY in the CSD fallback.
        .debug_selector(move || id.to_string())
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(t))
        .w(px(46.0))
        .h_full()
        .min_h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(taskmanager_ui::theme_binding::fill(bg))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
        .text_color(taskmanager_ui::theme_binding::hsla(fg))
        .on_hover(cx.listener(move |v, is_hov: &bool, _win, cx| {
            v.set_hover(
                if *is_hov {
                    Some(Hover::Static(id))
                } else {
                    None
                },
                cx,
            );
        }))
        .on_click(move |_ev, window, cx| action(window, cx))
        .child(glyph)
}

/// GNOME Adwaita single Close (circular, ~22px, subtle surface + hairline). Fills
/// with libadwaita destructive red (#E01B24) on hover.
fn close_circle(
    t: &Theme,
    hovered: Option<&Hover>,
    action: fn(&mut Window, &mut App),
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let is_hov = hovered == Some(&Hover::Static("wnd-close"));
    let (bg, fg) = if is_hov {
        (Color::from_hex(0xe01b24), Color::WHITE)
    } else {
        (t.sidebar_card_bg, t.fg)
    };
    // WCAG hit pad: invisible 28×28 footprint wraps the 22px visual circle. The
    // close button quits the app, so a generous click target matters most here.
    div()
        .id("wnd-close")
        // Zero-cost debug tag (no-op in release) so render tests can confirm the
        // GNOME AdwaitaClose button renders ONLY in the CSD fallback.
        .debug_selector(|| "wnd-close".to_string())
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(t))
        .mr(taskmanager_ui::theme_binding::length(tokens::SPACE_10))
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_hover(cx.listener(move |v, is_hov: &bool, _win, cx| {
            v.set_hover(
                if *is_hov {
                    Some(Hover::Static("wnd-close"))
                } else {
                    None
                },
                cx,
            );
        }))
        .on_click(move |_ev, window, cx| action(window, cx))
        .child(
            div()
                .size(px(22.0))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(taskmanager_ui::theme_binding::fill(bg))
                .border_1()
                .border_color(taskmanager_ui::theme_binding::hsla(t.border))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(fg))
                .child("\u{2715}"),
        )
}

// ── window-control actions ───────────────────────────────────────────────
fn close_and_quit(window: &mut Window, cx: &mut App) {
    window.remove_window();
    cx.quit();
}

/// The tray owns process termination once it is available. Closing the main
/// window only minimizes it, preserving the window/root entity for a later
/// tray or single-instance activation.
fn minimize_to_tray(window: &mut Window, _cx: &mut App) {
    window.minimize_window();
}
fn minimize_window(window: &mut Window, _cx: &mut App) {
    window.minimize_window();
}
fn zoom_window(window: &mut Window, _cx: &mut App) {
    // gpui 0.2.2 has no Linux maximize API; on macOS this toggles zoom.
    window.titlebar_double_click();
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_chrome_tests.rs"]
mod tests;
