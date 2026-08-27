//! Iced Settings modal: grouped preference controls (General / Appearance /
//! Fonts / System / Notifications / Units) persisted through the shared
//! background config coordinator.
//!
//! This is a frontend-local surface (like GPUI's Settings dialog): the shell
//! owns page routing and overlays, while preferences are per-frontend
//! presentation state. Every control consumes the owned inputs component
//! family (`crate::ui::components`) — segmented groups, switches, sliders,
//! selects — so the modal stays fully keyboard-reachable through the crate
//! focus shell, and every label/color comes from the neutral theme snapshot
//! or the local dictionary — never a literal. The shortcut legend under
//! General reads the same Iced binding declaration the F1 help overlay
//! renders, and the text-rendering row — Iced exposes no portable
//! text-raster mode — is an explicit unavailable state, never a dead
//! selector.

use iced::widget::{column, scrollable};
use iced::{Element, Length};
use taskmanager_theme::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, FontAvailability, FontChoice, FontRole, Theme, tokens,
};

use crate::app::{DeviceKind, FocusTarget, Message, PresentationPreferences, SettingsChange};
use crate::i18n::{self, Key, Language};

use super::components::IcedElement;
use super::components::{segmented, select, slider, switch};
use super::overlays::modal_overlay;

mod controls;
mod shortcuts;

use controls::*;
use shortcuts::shortcut_section;

/// The scrollable body height (px contract): the grouped page needs more
/// vertical room than the legacy 420px strip; the modal panel keeps its
/// fixed 680px width and the body fills it.
const SETTINGS_SCROLL_HEIGHT: f32 = 520.0;

fn is_system_font_token(token: &str) -> bool {
    token.eq_ignore_ascii_case("system")
}

fn is_bundled_font_token(token: &str, bundled_family: &str) -> bool {
    token.is_empty()
        || token.eq_ignore_ascii_case(bundled_family)
        || (bundled_family.eq_ignore_ascii_case(FONT_MISANS_VF)
            && token.eq_ignore_ascii_case(FONT_ROBOTO_MONO))
}

/// The focus stop for one settings control; each control is a single stop,
/// so every section name maps to one stable operation id
/// (`iced-settings-choice-{section}-0`).
fn target(section: &'static str) -> FocusTarget {
    FocusTarget::SettingsChoice { section, index: 0 }
}

/// Render the settings modal for the current app state. Active controls
/// read the persisted `Config` through the renderer's immutable projection
/// (updated on every settings change), so the view never performs I/O.
pub(super) fn render(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let appear = app.modal_appear_progress();
    let theme_snapshot = app.theme();
    let language = app.language();
    let prefs = app.preferences();

    let groups: Vec<IcedElement<'_>> = vec![
        general_group(theme_snapshot, language, prefs),
        appearance_group(theme_snapshot, language, app, prefs),
        fonts_group(theme_snapshot, language, app, prefs),
        system_group(theme_snapshot, language, prefs),
        notifications_group(theme_snapshot, prefs),
        units_group(theme_snapshot, language, prefs),
    ];

    modal_overlay(
        theme_snapshot,
        i18n::t(language, Key::Settings),
        taskmanager_application::i18n::t("settings.persist_hint"),
        scrollable(column(groups).spacing(f32::from(tokens::SPACE_12)))
            .height(Length::Fixed(SETTINGS_SCROLL_HEIGHT))
            .width(Length::Fill)
            .into(),
        appear,
    )
}

/// One semantic group: a strong header over a column of rows.
fn group<'a>(
    theme_snapshot: &'a Theme,
    key: &'static str,
    rows: Vec<IcedElement<'a>>,
) -> IcedElement<'a> {
    column(vec![
        group_header(theme_snapshot, taskmanager_application::i18n::t(key)),
        column(rows).spacing(f32::from(tokens::SPACE_2)).into(),
    ])
    .spacing(f32::from(tokens::SPACE_6))
    .into()
}

/// General: language, the declaration-driven shortcut legend, startup page.
fn general_group<'a>(
    theme_snapshot: &'a Theme,
    language: Language,
    prefs: &PresentationPreferences,
) -> IcedElement<'a> {
    let language_choices = language_choices();
    let startup_choices = startup_choices(language);
    group(
        theme_snapshot,
        "settings.group_general",
        vec![
            setting_row(
                taskmanager_application::i18n::t("settings.language"),
                boxed(
                    220.0,
                    select(
                        theme_snapshot,
                        target("language"),
                        language_choices,
                        language_choices.iter().find(|choice| choice.0 == language),
                        i18n::t(language, Key::Language),
                        |choice: &LanguageChoice| {
                            Message::SettingsChanged(SettingsChange::Language(choice.0))
                        },
                    ),
                ),
            ),
            setting_row(
                taskmanager_application::i18n::t("settings.startup_page"),
                boxed(
                    260.0,
                    select(
                        theme_snapshot,
                        target("startup-page"),
                        startup_choices,
                        startup_selected(startup_choices, &prefs.startup_page),
                        taskmanager_application::i18n::t("settings.startup_page"),
                        |choice: &StartupChoice| {
                            Message::SettingsChanged(SettingsChange::StartupPage(choice.token))
                        },
                    ),
                ),
            ),
            section_caption(
                theme_snapshot,
                taskmanager_application::i18n::t("settings.keyboard"),
            ),
            shortcut_section(theme_snapshot),
        ],
    )
}

/// Appearance: interface size, density, skin, mode, high contrast, motion.
fn appearance_group<'a>(
    theme_snapshot: &'a Theme,
    language: Language,
    app: &crate::IcedApp,
    prefs: &PresentationPreferences,
) -> IcedElement<'a> {
    let ui_size = app.ui_size();
    group(
        theme_snapshot,
        "settings.group_appearance",
        vec![
            setting_row(
                taskmanager_application::i18n::t("settings.ui_size"),
                segmented(
                    theme_snapshot,
                    target("ui-size"),
                    &ui_size_choices(),
                    ui_size_value(ui_size),
                    |value| {
                        Message::SettingsChanged(SettingsChange::UiSize(ui_size_for_value(value)))
                    },
                ),
            ),
            setting_row(
                i18n::t(language, Key::Density),
                segmented(
                    theme_snapshot,
                    target("density"),
                    &density_choices(language),
                    density_value(prefs.density.eq_ignore_ascii_case("Compact")),
                    |value| Message::SettingsChanged(SettingsChange::CompactDensity(value == 1)),
                ),
            ),
            setting_row(
                i18n::t(language, Key::Skin),
                segmented(
                    theme_snapshot,
                    target("skin"),
                    &skin_choices(),
                    skin_value(&prefs.skin),
                    |value| Message::SettingsChanged(SettingsChange::Skin(skin_for_value(value))),
                ),
            ),
            setting_row(
                i18n::t(language, Key::Mode),
                segmented(
                    theme_snapshot,
                    target("mode"),
                    &mode_choices(language),
                    mode_value(&prefs.mode),
                    |value| Message::SettingsChanged(SettingsChange::Mode(mode_for_value(value))),
                ),
            ),
            setting_row(
                i18n::t(language, Key::HighContrast),
                switch(theme_snapshot, target("hc"), prefs.hc, false, |on| {
                    Message::SettingsChanged(SettingsChange::HighContrast(on))
                }),
            ),
            setting_row(
                taskmanager_application::i18n::t("settings.motion"),
                segmented(
                    theme_snapshot,
                    target("motion"),
                    &motion_choices(),
                    motion_value(&prefs.motion),
                    |value| {
                        Message::SettingsChanged(SettingsChange::Motion(motion_for_value(value)))
                    },
                ),
            ),
        ],
    )
}

/// Fonts: UI/mono source segments plus installed-family selects, and the
/// explicitly-unavailable text-rendering row (Iced has no portable
/// text-raster mode; platform default is the only state and it is shown,
/// not toggled).
fn fonts_group<'a>(
    theme_snapshot: &'a Theme,
    language: Language,
    app: &'a crate::IcedApp,
    prefs: &PresentationPreferences,
) -> IcedElement<'a> {
    let availability = app.font_availability();
    group(
        theme_snapshot,
        "settings.group_fonts",
        vec![
            setting_row(
                taskmanager_application::i18n::t("settings.ui_font"),
                segmented(
                    theme_snapshot,
                    target("ui-font"),
                    &font_choice_choices(language),
                    font_choice_value(
                        is_system_font_token(&prefs.ui_font),
                        is_bundled_font_token(&prefs.ui_font, FONT_MISANS_VF),
                    ),
                    |value| {
                        Message::SettingsChanged(SettingsChange::UiFont(font_choice_for_value(
                            value,
                        )))
                    },
                ),
            ),
            setting_row(
                i18n::t(language, Key::FontFamily),
                boxed(
                    260.0,
                    family_select(
                        theme_snapshot,
                        FontRole::Ui,
                        &prefs.ui_font,
                        FONT_MISANS_VF,
                        availability,
                        language,
                    ),
                ),
            ),
            setting_row(
                taskmanager_application::i18n::t("settings.mono_font"),
                segmented(
                    theme_snapshot,
                    target("mono-font"),
                    &font_choice_choices(language),
                    font_choice_value(
                        is_system_font_token(&prefs.mono_font),
                        is_bundled_font_token(&prefs.mono_font, FONT_ROBOTO_MONO),
                    ),
                    |value| {
                        Message::SettingsChanged(SettingsChange::MonoFont(font_choice_for_value(
                            value,
                        )))
                    },
                ),
            ),
            setting_row(
                i18n::t(language, Key::FontFamily),
                boxed(
                    260.0,
                    family_select(
                        theme_snapshot,
                        FontRole::Mono,
                        &prefs.mono_font,
                        FONT_ROBOTO_MONO,
                        availability,
                        language,
                    ),
                ),
            ),
            setting_row(
                taskmanager_application::i18n::t("settings.text_rendering"),
                static_value(
                    theme_snapshot,
                    taskmanager_application::i18n::t("settings.text_default"),
                ),
            ),
            hint_line(
                theme_snapshot,
                // Frontend-neutral wording: the GPUI-specific key belongs to the
                // GPUI settings page; Iced has no text-raster variant at all.
                taskmanager_application::i18n::t("settings.text_rendering_unavailable_generic"),
            ),
        ],
    )
}

/// The installed-font family select for one font role. Options borrow the
/// app-owned availability catalog (they outlive the render frame); a token
/// already covered by the System/Bundled segment selects nothing, and an
/// empty catalog renders the select's inert placeholder surface.
fn family_select<'a>(
    theme_snapshot: &'a Theme,
    role: FontRole,
    token: &str,
    bundled_family: &'static str,
    availability: &'a FontAvailability,
    language: Language,
) -> IcedElement<'a> {
    let options = availability.custom_families();
    // The selected option is borrowed from the app-owned catalog slice (the
    // control needs it for the element's lifetime); `choice_for` canonicalizes
    // the persisted token onto the same catalog spelling, so the join by
    // equality cannot miss a real custom family.
    let selected = if is_system_font_token(token) || is_bundled_font_token(token, bundled_family) {
        None
    } else {
        availability
            .choice_for(token)
            .and_then(|choice| match choice {
                FontChoice::Custom(family) => Some(family),
                FontChoice::System | FontChoice::Bundled => None,
            })
            .and_then(|family| options.iter().find(|option| **option == family))
    };
    let section = match role {
        FontRole::Ui => "ui-font-family",
        FontRole::Mono => "mono-font-family",
    };
    select(
        theme_snapshot,
        target(section),
        options,
        selected,
        i18n::t(language, Key::FontChoose),
        move |family: &&'static str| {
            let choice = FontChoice::Custom(family);
            Message::SettingsChanged(match role {
                FontRole::Ui => SettingsChange::UiFont(choice),
                FontRole::Mono => SettingsChange::MonoFont(choice),
            })
        },
    )
}

/// The shared i18n key of one device-visibility row label.
fn device_label_key(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Cpu => "settings.show_cpu",
        DeviceKind::Memory => "settings.show_memory",
        DeviceKind::Disks => "settings.show_disks",
        DeviceKind::Network => "settings.show_network",
        DeviceKind::NetworkWired => "settings.show_network_wired",
        DeviceKind::NetworkWireless => "settings.show_network_wireless",
        DeviceKind::NetworkVpn => "settings.show_network_vpn",
        DeviceKind::NetworkVirtual => "settings.show_network_virtual",
        DeviceKind::NetworkOther => "settings.show_network_other",
        DeviceKind::Gpus => "settings.show_gpus",
    }
}

/// System: device visibility switches, refresh slider, graph options,
/// zero-value graying, continuous history.
fn system_group<'a>(
    theme_snapshot: &'a Theme,
    _language: Language,
    prefs: &PresentationPreferences,
) -> IcedElement<'a> {
    let mut rows: Vec<IcedElement<'a>> = Vec::new();
    rows.push(section_caption(
        theme_snapshot,
        taskmanager_application::i18n::t("settings.devices"),
    ));
    for kind in DeviceKind::ALL {
        rows.push(setting_row(
            taskmanager_application::i18n::t(device_label_key(kind)),
            switch(
                theme_snapshot,
                target(kind.key()),
                prefs.device_visible(kind),
                false,
                move |on| Message::SettingsChanged(SettingsChange::ShowDevice(kind, on)),
            ),
        ));
    }
    rows.push(setting_row(
        taskmanager_application::i18n::t("settings.refresh_interval"),
        slider(
            theme_snapshot,
            target("refresh"),
            refresh_range(),
            REFRESH_STEP_S,
            prefs.refresh_ms as f32 / 1000.0,
            |value| {
                Message::SettingsChanged(SettingsChange::RefreshInterval(refresh_value_to_ms(
                    value,
                )))
            },
            Some(refresh_label),
        ),
    ));
    rows.push(section_caption(
        theme_snapshot,
        taskmanager_application::i18n::t("settings.graph_settings"),
    ));
    rows.push(setting_row(
        taskmanager_application::i18n::t("settings.graph_data_points"),
        slider(
            theme_snapshot,
            target("data-points"),
            graph_points_range(),
            GRAPH_POINTS_STEP,
            prefs.graph_data_points as f32,
            |value| {
                Message::SettingsChanged(SettingsChange::GraphDataPoints(graph_points_for_value(
                    value,
                )))
            },
            Some(graph_points_label),
        ),
    ));
    rows.push(setting_row(
        taskmanager_application::i18n::t("settings.network_dynamic_scaling"),
        switch(
            theme_snapshot,
            target("net-scaling"),
            prefs.network_dynamic_scaling,
            false,
            |on| Message::SettingsChanged(SettingsChange::NetworkDynamicScaling(on)),
        ),
    ));
    rows.push(setting_row(
        taskmanager_application::i18n::t("settings.zero_values"),
        switch(
            theme_snapshot,
            target("zero-values"),
            prefs.gray_zero_values,
            false,
            |on| Message::SettingsChanged(SettingsChange::GrayZeroValues(on)),
        ),
    ));
    rows.push(hint_line(
        theme_snapshot,
        taskmanager_application::i18n::t("settings.gray_zero_values_hint"),
    ));
    rows.push(setting_row(
        taskmanager_application::i18n::t("settings.history_persistence"),
        switch(
            theme_snapshot,
            target("continuous-history"),
            prefs.history_persistence,
            false,
            |on| Message::SettingsChanged(SettingsChange::ContinuousHistory(on)),
        ),
    ));
    rows.push(hint_line(
        theme_snapshot,
        taskmanager_application::i18n::t("settings.history_persistence_detail"),
    ));
    group(theme_snapshot, "settings.group_system", rows)
}

/// Notifications: desktop-notification opt-in and quiet-hour boundaries.
fn notifications_group<'a>(
    theme_snapshot: &'a Theme,
    prefs: &PresentationPreferences,
) -> IcedElement<'a> {
    let hours = quiet_hours();
    group(
        theme_snapshot,
        "settings.group_notifications",
        vec![
            setting_row(
                taskmanager_application::i18n::t("settings.desktop_notifications"),
                switch(
                    theme_snapshot,
                    target("desktop-notifications"),
                    prefs.notify_enabled,
                    false,
                    |on| Message::SettingsChanged(SettingsChange::DesktopNotifications(on)),
                ),
            ),
            hint_line(
                theme_snapshot,
                taskmanager_application::i18n::t("settings.desktop_notifications_hint"),
            ),
            setting_row(
                taskmanager_application::i18n::t("settings.quiet_hours_start"),
                boxed(
                    200.0,
                    select(
                        theme_snapshot,
                        target("quiet-hours-start"),
                        hours,
                        hours.get(usize::from(prefs.quiet_start)),
                        taskmanager_application::i18n::t("settings.quiet_hours_start"),
                        |hour: &QuietHour| {
                            Message::SettingsChanged(SettingsChange::QuietHoursStart(hour.0))
                        },
                    ),
                ),
            ),
            setting_row(
                taskmanager_application::i18n::t("settings.quiet_hours_end"),
                boxed(
                    200.0,
                    select(
                        theme_snapshot,
                        target("quiet-hours-end"),
                        hours,
                        hours.get(usize::from(prefs.quiet_end)),
                        taskmanager_application::i18n::t("settings.quiet_hours_end"),
                        |hour: &QuietHour| {
                            Message::SettingsChanged(SettingsChange::QuietHoursEnd(hour.0))
                        },
                    ),
                ),
            ),
        ],
    )
}

/// One unit-toggle surface row: the two i18n labels, the two segmented
/// focus ids, the two current preferences, and the two change constructors.
type UnitSurfaceRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    bool,
    bool,
    fn(bool) -> SettingsChange,
    fn(bool) -> SettingsChange,
);

/// Units: per-surface (memory / drive / network) Bytes-vs-Bits and
/// Base-2-vs-Base-10 segment pairs.
fn units_group<'a>(
    theme_snapshot: &'a Theme,
    language: Language,
    prefs: &PresentationPreferences,
) -> IcedElement<'a> {
    let mut rows: Vec<IcedElement<'a>> = Vec::new();
    let surfaces: [UnitSurfaceRow; 3] = [
        (
            "settings.memory_usage_unit",
            "settings.memory_usage_base",
            "memory-unit",
            "memory-base",
            prefs.memory_use_bytes,
            prefs.memory_use_base2,
            SettingsChange::MemoryBytes,
            SettingsChange::MemoryBase2,
        ),
        (
            "settings.drive_usage_unit",
            "settings.drive_usage_base",
            "drive-unit",
            "drive-base",
            prefs.drive_use_bytes,
            prefs.drive_use_base2,
            SettingsChange::DriveBytes,
            SettingsChange::DriveBase2,
        ),
        (
            "settings.network_usage_unit",
            "settings.network_usage_base",
            "network-unit",
            "network-base",
            prefs.network_use_bytes,
            prefs.network_use_base2,
            SettingsChange::NetworkBytes,
            SettingsChange::NetworkBase2,
        ),
    ];
    for (
        unit_key,
        base_key,
        unit_section,
        base_section,
        use_bytes,
        use_base2,
        bytes_change,
        base2_change,
    ) in surfaces
    {
        rows.push(section_caption(
            theme_snapshot,
            taskmanager_application::i18n::t(unit_key),
        ));
        rows.push(setting_row(
            taskmanager_application::i18n::t(unit_key),
            segmented(
                theme_snapshot,
                target(unit_section),
                &unit_bytes_choices(language),
                unit_toggle_value(use_bytes),
                move |value| Message::SettingsChanged(bytes_change(value == 0)),
            ),
        ));
        rows.push(setting_row(
            taskmanager_application::i18n::t(base_key),
            segmented(
                theme_snapshot,
                target(base_section),
                &unit_base_choices(),
                unit_toggle_value(use_base2),
                move |value| Message::SettingsChanged(base2_change(value == 0)),
            ),
        ));
    }
    group(theme_snapshot, "settings.group_units", rows)
}

#[cfg(test)]
#[path = "../../tests/gui/ui/settings_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/settings_upgrade_tests.rs"]
mod upgrade_tests;
