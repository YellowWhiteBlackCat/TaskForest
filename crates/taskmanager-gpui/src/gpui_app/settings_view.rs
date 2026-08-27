//! Settings modal content: skin picker (GNOME/KDE/Windows/macOS), light/dark,
//! high-contrast, and Mission Center-compatible Performance units. Rendered as
//! the **content** of the own modal dialog
//! (`taskmanager_ui::overlays::dialog` pushed into a LayerStack, see
//! [`crate::gpui_app::elements::dialog_overlay`]) — the Dialog supplies the panel
//! chrome (bg / border / radius / shadow), the title header, and the close (X) button;
//! this fn returns just the sections. All controls mutate `RootView.theme` via the
//! entity handle, so the whole app re-skins live on the next frame.

use gpui::{Context, Div, Entity, IntoElement, ParentElement, SharedString, Styled, div, px};
use std::collections::HashMap;

use taskmanager_ui::inputs::select::{SelectOption, select};
use taskmanager_ui::inputs::slider::SliderState;
use taskmanager_ui::inputs::switch::SwitchState;
use taskmanager_ui::primitives::section_header::SectionHeader;

use self::density::density_row;
use self::ui_size::ui_size_row;
use crate::core::config::{
    STARTUP_PAGE_PERFORMANCE, STARTUP_PAGE_PROCESSES, STARTUP_PAGE_REMEMBER,
};
use crate::gpui_app::elements::pill;
use crate::gpui_app::formatting::DisplayUnits;
use crate::gpui_app::graph::GraphSettings;
use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::theme::tokens;
use crate::gpui_app::theme::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, FontAvailability, FontChoice, FontPreference, FontRole, Skin,
    Theme,
    tokens::{RowDensity, UiSize},
};
use crate::i18n;

mod appearance;
mod density;
mod fonts;
mod graphs;
mod history;
mod notifications;
mod shortcuts;
mod ui_size;
mod units;
mod zero_values;

use self::appearance::mode_row;
use self::devices::{DeviceVisibility, devices_row};
use self::graphs::graph_options_group;
pub(crate) use self::graphs::init_data_points_slider;
use self::history::history_persistence_row;
use self::notifications::{notify_row, quiet_hours_rows};
use self::refresh::refresh_row;
use self::units::units_group;
use self::zero_values::zero_values_row;

mod devices;
pub(crate) mod refresh;

/// Render the Settings modal content, organized as Zed-style semantic groups —
/// **General** (Language, Keyboard shortcuts, Startup page), **Appearance**
/// (Skin, Light/Dark/EyeForest, High contrast), **Fonts** (interface + monospace faces,
/// Text rendering), **System** (Devices, Apps, Performance). Each `group`
/// and **Units** (Memory/Drive/Network Bytes/Bits and Base 2/Base 10).
/// carries a bold title; inside it, the familiar titled `section`s keep their
/// dim captions (Skin GNOME/KDE/Windows/macOS pills, Light/Dark, Language,
/// Fonts UI + monospace, Text rendering platform-default/subpixel/grayscale,
/// Keyboard shortcuts, Devices per-category toggles, Apps zero-value styling,
/// Performance refresh-interval slider, High contrast switch).
///
/// The returned element is the Dialog **content** only; the wrapping
/// [`crate::gpui_app::elements::dialog_overlay`] supplies the panel chrome
/// (bg/border/radius/shadow), the title header, and the close (X) button. Every
/// control mutates `RootView.theme` (or the collector interval) via the entity
/// handle, so the whole app re-skins live on the next frame.
///
/// `hovered` is the uniform hover slot root passes to every render (consumed by
/// the skin/mode pills' hover overlays). `refresh_secs` is the live collector
/// interval, re-read by `RootView::render` each frame and fed to the slider
/// readout. The `show_*` flags mirror the matching `RootView` fields and seed
/// each device toggle's initial state. `gray_zero_values` mirrors the Apps-page
/// zero-value preference. `font_pref` is the current font intent,
/// seeding the font pills (the theme's resolved families drive the active
/// highlight). `text_rendering` / `startup_page` are the window's live tokens
/// (`RootView.settings_text_rendering` / `settings_startup_page`), seeding the
/// matching pills. `slider_entity` is the window's own persistent
/// refresh-interval `Entity<SliderState>` (`RootView.settings_slider`).
/// All straight-through settings render inputs (design-debt #1 props
/// consolidation); `cx` stays an explicit render-lifetime handle.
pub(crate) struct SettingsViewProps<'a> {
    pub theme: &'a Theme,
    pub hovered: Option<&'a Hover>,
    pub refresh_secs: f32,
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disks: bool,
    pub show_network: bool,
    pub show_network_wired: bool,
    pub show_network_wireless: bool,
    pub show_network_vpn: bool,
    pub show_network_virtual: bool,
    pub show_network_other: bool,
    pub show_gpus: bool,
    pub units: DisplayUnits,
    pub graph_settings: GraphSettings,
    pub graph_points_slider: Entity<SliderState>,
    pub gray_zero_values: bool,
    pub notify_enabled: bool,
    pub history_persistence: bool,
    pub notify_quiet_start: u8,
    pub notify_quiet_end: u8,
    pub font_pref: FontPreference,
    pub font_availability: &'a FontAvailability,
    pub density: RowDensity,
    pub ui_size: UiSize,
    pub color_scheme: &'static str,
    pub text_rendering: &'static str,
    pub startup_page: SharedString,
    pub slider_entity: Entity<SliderState>,
    pub switches: &'a HashMap<&'static str, Entity<SwitchState>>,
}

pub(crate) fn render_settings(
    props: SettingsViewProps<'_>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let SettingsViewProps {
        theme: t,
        hovered,
        refresh_secs,
        show_cpu,
        show_memory,
        show_disks,
        show_network,
        show_network_wired,
        show_network_wireless,
        show_network_vpn,
        show_network_virtual,
        show_network_other,
        show_gpus,
        units,
        graph_settings,
        graph_points_slider,
        gray_zero_values,
        notify_enabled,
        history_persistence,
        notify_quiet_start,
        notify_quiet_end,
        font_pref,
        font_availability,
        density,
        ui_size,
        color_scheme,
        text_rendering,
        startup_page,
        slider_entity,
        switches,
    } = props;
    let ent = cx.entity();

    // NOTE: no outer chrome box / title row / Close button here — the wrapping
    // `dialog_overlay` (taskmanager_ui dialog) supplies them. This fn returns
    // just the grouped sections so the Dialog's chrome is the single visual frame.
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(20.0))
        .child(group(
            t,
            "settings.group_general",
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_12)
                .child(section(
                    t,
                    i18n::t("settings.language"),
                    language_row(t, ent.clone(), hovered, cx),
                ))
                .child(section(
                    t,
                    i18n::t("settings.keyboard"),
                    shortcuts::shortcut_grid(t),
                ))
                .child(section(
                    t,
                    i18n::t("settings.startup_page"),
                    startup_page_row(t, ent.clone(), startup_page),
                )),
        ))
        .child(group(
            t,
            "settings.group_appearance",
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_12)
                .child(section(
                    t,
                    i18n::t("settings.ui_size"),
                    ui_size_row(t, ent.clone(), ui_size, hovered, cx),
                ))
                .child(section(
                    t,
                    i18n::t("settings.density"),
                    density_row(t, ent.clone(), density, hovered, cx),
                ))
                .child(section(
                    t,
                    i18n::t("settings.skin"),
                    skin_row(t, ent.clone(), hovered, cx),
                ))
                .child(section(
                    t,
                    i18n::t("settings.appearance"),
                    mode_row(t, ent.clone(), color_scheme, hovered, cx),
                ))
                .child(hc_row(t, ent.clone(), switches, cx)),
        ))
        .child(group(
            t,
            "settings.group_fonts",
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_12)
                .child(section(
                    t,
                    i18n::t("settings.fonts"),
                    fonts::font_row(t, ent.clone(), font_pref, font_availability, hovered, cx),
                ))
                .child(section(
                    t,
                    i18n::t("settings.text_rendering"),
                    text_rendering_row(t, ent.clone(), text_rendering, hovered, cx),
                )),
        ))
        .child(group(
            t,
            "settings.group_system",
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_12)
                .child(section(
                    t,
                    i18n::t("settings.devices"),
                    devices_row(
                        t,
                        ent.clone(),
                        DeviceVisibility {
                            cpu: show_cpu,
                            memory: show_memory,
                            disks: show_disks,
                            network: show_network,
                            network_wired: show_network_wired,
                            network_wireless: show_network_wireless,
                            network_vpn: show_network_vpn,
                            network_virtual: show_network_virtual,
                            network_other: show_network_other,
                            gpus: show_gpus,
                        },
                        switches,
                        cx,
                    ),
                ))
                .child(section(
                    t,
                    i18n::t("settings.performance"),
                    refresh_row(t, refresh_secs, slider_entity, cx),
                ))
                .child(section(
                    t,
                    i18n::t("settings.graph_settings"),
                    graph_options_group(
                        t,
                        ent.clone(),
                        graph_settings,
                        graph_points_slider,
                        switches,
                        cx,
                    ),
                ))
                .child(section(
                    t,
                    i18n::t("settings.apps"),
                    zero_values_row(t, ent.clone(), gray_zero_values, switches, cx),
                ))
                .child(section(
                    t,
                    i18n::t("settings.history_persistence"),
                    history_persistence_row(t, ent.clone(), history_persistence, switches, cx),
                )),
        ))
        .child(group(
            t,
            "settings.group_notifications",
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_12)
                .child(section(
                    t,
                    i18n::t("settings.notifications"),
                    notify_row(t, ent.clone(), notify_enabled, switches, cx),
                ))
                .child(section(
                    t,
                    i18n::t("settings.quiet_hours"),
                    quiet_hours_rows(t, ent.clone(), notify_quiet_start, notify_quiet_end),
                )),
        ))
        .child(group(
            t,
            "settings.group_units",
            units_group(t, ent, units, hovered, cx),
        ))
}

/// A semantic group: a bold group title (with a hairline divider) above a
/// column of [`section`]s. The Settings dialog is organized into Zed-style
/// semantic groups — General / Appearance / Fonts / System — so the dialog
/// reads as a compact settings page instead of a flat list of eight sections.
///
/// `key` is the i18n key (the title resolves through [`i18n::t`]); it also
/// becomes the group title's element id and test debug selector, so headless
/// tests can assert the groups render and their vertical order without
/// depending on the localized label text.
fn group(t: &Theme, key: &'static str, content: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(
            SectionHeader::new(i18n::t(key).to_owned(), t.palette())
                .debug_selector(key)
                .render(),
        )
        .child(content)
}

/// One titled section: a dim caption (`label`) above the `content` block. The
/// shared layout primitive used by every Settings row (Skin / Appearance /
/// Devices / Performance).
fn section(t: &Theme, label: &str, content: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(t.fg_dim)
                .child(label.to_string()),
        )
        .child(content)
}

/// Skin chooser: a wrapping row of four pills (GNOME / KDE / Windows / macOS).
/// Each pill calls `RootView.theme.set_skin(...)` on click, so the whole app
/// re-skins on the next frame.
fn skin_row(
    t: &Theme,
    ent: Entity<RootView>,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(tokens::SPACE_6)
        .child(skin_pill(
            t,
            ent.clone(),
            "skin-gnome",
            "GNOME",
            Skin::Gnome,
            hovered,
            cx,
        ))
        .child(skin_pill(
            t,
            ent.clone(),
            "skin-kde",
            "KDE",
            Skin::Kde,
            hovered,
            cx,
        ))
        .child(skin_pill(
            t,
            ent.clone(),
            "skin-win",
            "Windows",
            Skin::Windows,
            hovered,
            cx,
        ))
        .child(skin_pill(
            t,
            ent,
            "skin-mac",
            "macOS",
            Skin::Macos,
            hovered,
            cx,
        ))
}

/// One skin pill, built on [`elements::pill`](crate::gpui_app::elements::pill).
/// Active when the current theme's skin equals `skin`; `on_click` applies the
/// skin, `on_hover` publishes `Hover::Static(id)` so the active/hover overlay
/// resolves identically to the list-page status pills.
fn skin_pill(
    t: &Theme,
    ent: Entity<RootView>,
    id: &'static str,
    label: &'static str,
    skin: Skin,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let active = t.skin == skin;
    let is_hov = hovered == Some(&Hover::Static(id));
    pill(
        t,
        id,
        label,
        active,
        is_hov,
        move |_win, cx| {
            ent.update(cx, |v, cx| {
                // RootView::set_skin re-resolves the font stack for the new
                // skin (its system family may not be installed on this host).
                v.set_skin(skin, cx);
            });
        },
        cx.listener(move |v, is_hov: &bool, _win, cx| {
            v.set_hover(
                if *is_hov {
                    Some(Hover::Static(id))
                } else {
                    None
                },
                cx,
            );
        }),
    )
}

/// Language chooser: two pills (English / 中文) bound to the per-window
/// [`RootView::set_language_preference`] path. Each pill updates the global
/// catalog and calls `cx.notify()`
/// so the whole shell re-renders localized on the next frame (titlebar tabs,
/// tooltips, and every Settings label resolve through [`i18n::t`] against the
/// new language). The active pill tracks [`i18n::current_language`]; hover
/// overlay mirrors the skin/mode pills.
///
/// Each language is labeled in its **own** tongue (`English` / `中文`) in BOTH
/// locales — the universal convention for language pickers, so a user who can't
/// read the current UI language can still recognize and pick their own. Hence
/// `settings.lang_en` / `settings.lang_zh` carry the same string in en.json and
/// zh.json.
fn language_row(
    t: &Theme,
    ent: Entity<RootView>,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    let cur = i18n::current_language();
    div()
        .flex()
        .flex_row()
        .gap(tokens::SPACE_6)
        .child(pill(
            t,
            "lang-en",
            i18n::t("settings.lang_en"),
            cur == i18n::Language::En,
            hovered == Some(&Hover::Static("lang-en")),
            {
                let ent = ent.clone();
                move |_win, cx| {
                    ent.update(cx, |_v, cx| {
                        _v.set_language_preference(i18n::Language::En, cx);
                    });
                }
            },
            cx.listener(move |v, is_hov: &bool, _win, cx| {
                v.set_hover(
                    if *is_hov {
                        Some(Hover::Static("lang-en"))
                    } else {
                        None
                    },
                    cx,
                );
            }),
        ))
        .child(pill(
            t,
            "lang-zh",
            i18n::t("settings.lang_zh"),
            cur == i18n::Language::Zh,
            hovered == Some(&Hover::Static("lang-zh")),
            move |_win, cx| {
                ent.update(cx, |_v, cx| {
                    _v.set_language_preference(i18n::Language::Zh, cx);
                });
            },
            cx.listener(move |v, is_hov: &bool, _win, cx| {
                v.set_hover(
                    if *is_hov {
                        Some(Hover::Static("lang-zh"))
                    } else {
                        None
                    },
                    cx,
                );
            }),
        ))
}
/// Text-rendering chooser: Platform default is the only active choice in the
/// published GPUI 0.2.2 build. Subpixel and grayscale remain visible as
/// disabled capability evidence because their API exists only in Zed's
/// in-tree GPUI fork; the token is normalized at startup and persistence so a
/// click cannot claim an effect the renderer did not apply.
mod rendering;
#[cfg(test)]
#[path = "../../tests/gui/gpui_app/settings_view/tests.rs"]
mod tests;
pub(crate) use rendering::{hc_row, text_rendering_row};
// No pixels, no filesystem, no process signals: a test window renders the
// settings content against a live RootView entity, and the Startup-page pills
// are activated through the REAL pipeline (tab-stop focus + keyboard click
// event → gpui-component Button on_click → token update). The grouping test
// asserts the four semantic group titles render top-to-bottom via the
// debug_selector the `group()` helper registers.

// ── startup-page chooser (remember last / performance / processes) ──────────

fn startup_page_row(
    t: &Theme,
    ent: Entity<RootView>,
    startup_page: SharedString,
) -> impl IntoElement {
    select(
        "startup-page",
        Some(startup_page),
        i18n::t("settings.startup_page"),
        vec![
            SelectOption::new(STARTUP_PAGE_REMEMBER, i18n::t("settings.startup_remember")),
            SelectOption::new(
                STARTUP_PAGE_PERFORMANCE,
                i18n::t("settings.startup_performance"),
            ),
            SelectOption::new(
                STARTUP_PAGE_PROCESSES,
                i18n::t("settings.startup_processes"),
            ),
        ],
        t.palette(),
        move |token, _win, cx| {
            ent.update(cx, |v, cx| {
                // The startup module seeds this per-window token from Config
                // and applies the fixed-page policy before this control is
                // rendered; the select only changes the preference state.
                v.set_startup_page_preference(token, cx);
            });
        },
    )
}
