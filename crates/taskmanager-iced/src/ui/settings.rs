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

use iced::widget::{column, text};
use iced::{Element, Length};
use taskmanager_theme::{FontAvailability, FontChoice, FontRole, Theme, tokens};

use crate::app::{FocusTarget, Message, PresentationPreferences, SettingsChange};
use crate::ui::components::{ButtonTone, glass_button, styled_scrollable};
use taskforest_core::core::config::DeviceVisibilityKey;
use taskmanager_application::i18n::Language;

use super::components::IcedElement;
use super::components::{label, section_header, segmented, select, slider, switch};
use super::overlays::modal_overlay;
use taskmanager_application::i18n::t;

mod controls;
mod privilege_center;
mod shortcuts;

use controls::*;
use privilege_center::privileges_group;
use shortcuts::shortcut_section;

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

    let mut groups: Vec<IcedElement<'_>> = vec![
        privileges_group(app),
        general_group(theme_snapshot, language, prefs),
        appearance_group(theme_snapshot, app, prefs),
        fonts_group(theme_snapshot, app, prefs),
        system_group(theme_snapshot, prefs),
        notifications_group(theme_snapshot, prefs),
        units_group(theme_snapshot, prefs),
    ];

    if app.first_run.view().info.is_some() {
        groups.insert(
            1,
            group(
                theme_snapshot,
                "settings.additional_setup",
                vec![
                    text(t("settings.additional_setup_detail"))
                        .size(f32::from(tokens::FONT_12))
                        .into(),
                    glass_button(
                        theme_snapshot,
                        FocusTarget::FirstRunOpen,
                        t("settings.additional_setup_open").to_owned(),
                        Message::FirstRun(crate::app::FirstRunMessage::Open),
                        ButtonTone::Primary,
                        false,
                    ),
                ],
            ),
        );
    }

    modal_overlay(
        theme_snapshot,
        t("chrome.settings"),
        t("settings.persist_hint"),
        styled_scrollable(column(groups).spacing(f32::from(tokens::SPACE_12)))
            .height(Length::Fill)
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
        section_header(theme_snapshot, t(key)),
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
                t("settings.language"),
                boxed(
                    220.0,
                    select(
                        theme_snapshot,
                        target("language"),
                        language_choices,
                        language_choices.iter().find(|choice| choice.0 == language),
                        t("settings.language"),
                        |choice: &LanguageChoice| {
                            Message::SettingsChanged(SettingsChange::Language(choice.0))
                        },
                    ),
                ),
            ),
            setting_row(
                t("settings.startup_page"),
                boxed(
                    260.0,
                    select(
                        theme_snapshot,
                        target("startup-page"),
                        startup_choices,
                        startup_selected(startup_choices, &prefs.startup_page),
                        t("settings.startup_page"),
                        |choice: &StartupChoice| {
                            Message::SettingsChanged(SettingsChange::StartupPage(choice.token))
                        },
                    ),
                ),
            ),
            label(theme_snapshot, t("settings.keyboard"), true),
            shortcut_section(theme_snapshot),
        ],
    )
}

/// Appearance: interface size, density, skin, mode, high contrast, motion.
fn appearance_group<'a>(
    theme_snapshot: &'a Theme,
    app: &crate::IcedApp,
    prefs: &PresentationPreferences,
) -> IcedElement<'a> {
    let ui_size = app.ui_size();
    group(
        theme_snapshot,
        "settings.group_appearance",
        vec![
            setting_row(
                t("settings.ui_size"),
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
                t("settings.density"),
                segmented(
                    theme_snapshot,
                    target("density"),
                    &density_choices(),
                    density_value(&prefs.density),
                    |value| Message::SettingsChanged(SettingsChange::CompactDensity(value == 1)),
                ),
            ),
            setting_row(
                t("settings.skin"),
                segmented(
                    theme_snapshot,
                    target("skin"),
                    &skin_choices(),
                    skin_value(&prefs.skin),
                    |value| Message::SettingsChanged(SettingsChange::Skin(skin_for_value(value))),
                ),
            ),
            setting_row(
                t("settings.mode"),
                segmented(
                    theme_snapshot,
                    target("mode"),
                    &mode_choices(),
                    mode_value(&prefs.mode),
                    |value| Message::SettingsChanged(SettingsChange::Mode(mode_for_value(value))),
                ),
            ),
            setting_row(
                t("settings.high_contrast"),
                switch(*theme_snapshot, target("hc"), prefs.hc, false, |on| {
                    Message::SettingsChanged(SettingsChange::HighContrast(on))
                }),
            ),
            setting_row(
                t("settings.motion"),
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
    app: &'a crate::IcedApp,
    prefs: &PresentationPreferences,
) -> IcedElement<'a> {
    let availability = app.font_availability();
    group(
        theme_snapshot,
        "settings.group_fonts",
        vec![
            setting_row(
                t("settings.ui_font"),
                segmented(
                    theme_snapshot,
                    target("ui-font"),
                    &font_choice_choices(),
                    font_choice_value(&prefs.ui_font, FontRole::Ui, availability),
                    |value| {
                        Message::SettingsChanged(SettingsChange::UiFont(font_choice_for_value(
                            value,
                        )))
                    },
                ),
            ),
            setting_row(
                t("settings.font_family"),
                boxed(
                    260.0,
                    family_select(theme_snapshot, FontRole::Ui, &prefs.ui_font, availability),
                ),
            ),
            setting_row(
                t("settings.mono_font"),
                segmented(
                    theme_snapshot,
                    target("mono-font"),
                    &font_choice_choices(),
                    font_choice_value(&prefs.mono_font, FontRole::Mono, availability),
                    |value| {
                        Message::SettingsChanged(SettingsChange::MonoFont(font_choice_for_value(
                            value,
                        )))
                    },
                ),
            ),
            setting_row(
                t("settings.font_family"),
                boxed(
                    260.0,
                    family_select(
                        theme_snapshot,
                        FontRole::Mono,
                        &prefs.mono_font,
                        availability,
                    ),
                ),
            ),
            setting_row(
                t("settings.text_rendering"),
                static_value(theme_snapshot, t("settings.text_default")),
            ),
            label(
                theme_snapshot,
                // Frontend-neutral wording: the GPUI-specific key belongs to the
                // GPUI settings page; Iced has no text-raster variant at all.
                t("settings.text_rendering_unavailable_generic"),
                true,
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
    availability: &'a FontAvailability,
) -> IcedElement<'a> {
    let options = availability.custom_families();
    // The theme owner's codec decides what a persisted token means, so the
    // segment row and this select share one resolution instead of two
    // hand-maintained string tests. `choice_for` canonicalizes the token onto
    // the catalog spelling, so the join by equality cannot miss a family.
    let selected = match FontChoice::from_config_token(token, role, availability) {
        FontChoice::Custom(family) => options.iter().find(|option| **option == family),
        FontChoice::System | FontChoice::Bundled => None,
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
        t("settings.font_choose"),
        move |family: &&'static str| {
            let choice = FontChoice::Custom(family);
            Message::SettingsChanged(match role {
                FontRole::Ui => SettingsChange::UiFont(choice),
                FontRole::Mono => SettingsChange::MonoFont(choice),
            })
        },
    )
}

/// System: device visibility switches, refresh slider, graph options,
/// zero-value graying, continuous history.
fn system_group<'a>(theme_snapshot: &'a Theme, prefs: &PresentationPreferences) -> IcedElement<'a> {
    let mut rows: Vec<IcedElement<'a>> = Vec::new();
    rows.push(label(theme_snapshot, t("settings.devices"), true));
    for kind in DeviceVisibilityKey::ALL {
        rows.push(setting_row(
            t(kind.i18n_key()),
            switch(
                *theme_snapshot,
                target(kind.stable_id()),
                prefs.device_visible(kind),
                false,
                move |on| Message::SettingsChanged(SettingsChange::ShowDevice(kind, on)),
            ),
        ));
    }
    rows.push(setting_row(
        t("settings.refresh_interval"),
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
    rows.push(label(theme_snapshot, t("settings.graph_settings"), true));
    rows.push(setting_row(
        t("settings.graph_data_points"),
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
        t("settings.network_dynamic_scaling"),
        switch(
            *theme_snapshot,
            target("net-scaling"),
            prefs.network_dynamic_scaling,
            false,
            |on| Message::SettingsChanged(SettingsChange::NetworkDynamicScaling(on)),
        ),
    ));
    rows.push(setting_row(
        t("settings.zero_values"),
        switch(
            *theme_snapshot,
            target("zero-values"),
            prefs.gray_zero_values,
            false,
            |on| Message::SettingsChanged(SettingsChange::GrayZeroValues(on)),
        ),
    ));
    rows.push(label(
        theme_snapshot,
        t("settings.gray_zero_values_hint"),
        true,
    ));
    rows.push(setting_row(
        t("settings.history_persistence"),
        switch(
            *theme_snapshot,
            target("continuous-history"),
            prefs.history_persistence,
            false,
            |on| Message::SettingsChanged(SettingsChange::ContinuousHistory(on)),
        ),
    ));
    rows.push(label(
        theme_snapshot,
        t("settings.history_persistence_detail"),
        true,
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
                t("settings.desktop_notifications"),
                switch(
                    *theme_snapshot,
                    target("desktop-notifications"),
                    prefs.notify_enabled,
                    false,
                    |on| Message::SettingsChanged(SettingsChange::DesktopNotifications(on)),
                ),
            ),
            label(
                theme_snapshot,
                t("settings.desktop_notifications_hint"),
                true,
            ),
            setting_row(
                t("settings.quiet_hours_start"),
                boxed(
                    200.0,
                    select(
                        theme_snapshot,
                        target("quiet-hours-start"),
                        hours,
                        hours.get(usize::from(prefs.quiet_start)),
                        t("settings.quiet_hours_start"),
                        |hour: &QuietHour| {
                            Message::SettingsChanged(SettingsChange::QuietHoursStart(hour.0))
                        },
                    ),
                ),
            ),
            setting_row(
                t("settings.quiet_hours_end"),
                boxed(
                    200.0,
                    select(
                        theme_snapshot,
                        target("quiet-hours-end"),
                        hours,
                        hours.get(usize::from(prefs.quiet_end)),
                        t("settings.quiet_hours_end"),
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
fn units_group<'a>(theme_snapshot: &'a Theme, prefs: &PresentationPreferences) -> IcedElement<'a> {
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
        rows.push(label(theme_snapshot, t(unit_key), true));
        rows.push(setting_row(
            t(unit_key),
            segmented(
                theme_snapshot,
                target(unit_section),
                &unit_bytes_choices(),
                unit_toggle_value(use_bytes),
                move |value| Message::SettingsChanged(bytes_change(value == 0)),
            ),
        ));
        rows.push(setting_row(
            t(base_key),
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

#[cfg(test)]
#[path = "../../tests/gui/ui/input_interaction_tests.rs"]
mod interaction_tests;
