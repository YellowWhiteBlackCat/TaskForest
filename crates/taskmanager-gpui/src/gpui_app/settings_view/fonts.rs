//! Interface and monospace font-choice settings.

use gpui::{Context, Div, Entity, IntoElement, ParentElement, SharedString, Styled, div};
use taskmanager_ui::inputs::select::{SelectOption, select};

use super::{
    FONT_MISANS_VF, FONT_ROBOTO_MONO, FontAvailability, FontChoice, FontPreference, FontRole,
    Hover, RootView, Theme, i18n, pill, tokens,
};

struct FontChoiceRowProps<'a> {
    theme: &'a Theme,
    ent: Entity<RootView>,
    role: FontRole,
    caption: &'a str,
    bundled_label: &'static str,
    bundled_id: &'static str,
    system_id: &'static str,
    choice: FontChoice,
    availability: &'a FontAvailability,
    hovered: Option<&'a Hover>,
}

fn font_choice_row(props: FontChoiceRowProps<'_>, cx: &mut Context<RootView>) -> Div {
    let FontChoiceRowProps {
        theme,
        ent,
        role,
        caption,
        bundled_label,
        bundled_id,
        system_id,
        choice,
        availability,
        hovered,
    } = props;
    let family_choices: Vec<(&'static str, SharedString)> = availability
        .custom_families()
        .iter()
        .copied()
        .map(|family| (family, SharedString::from(format!("family:{family}"))))
        .collect();
    let selected_value = match choice {
        FontChoice::Bundled => SharedString::from("bundled"),
        FontChoice::System => SharedString::from("system"),
        FontChoice::Custom(family) => SharedString::from(format!("family:{family}")),
    };
    let mut options = vec![
        SelectOption::new("bundled", bundled_label),
        SelectOption::new("system", i18n::t("settings.font_system")),
    ];
    options.extend(
        family_choices
            .iter()
            .map(|(family, value)| SelectOption::new(value.clone(), *family)),
    );
    let family_choices_for_change = family_choices.clone();
    let select_id = match role {
        FontRole::Ui => "font-ui-family",
        FontRole::Mono => "font-mono-family",
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .text_size(tokens::FONT_13)
                .text_color(theme.fg)
                .child(caption.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap(tokens::SPACE_6)
                .child(font_pill(
                    FontPillProps {
                        theme,
                        ent: ent.clone(),
                        role,
                        id: system_id,
                        label: i18n::t("settings.font_system"),
                        active: choice == FontChoice::System,
                        hovered,
                        choice: FontChoice::System,
                    },
                    cx,
                ))
                .child(font_pill(
                    FontPillProps {
                        theme,
                        ent: ent.clone(),
                        role,
                        id: bundled_id,
                        label: bundled_label,
                        active: choice == FontChoice::Bundled,
                        hovered,
                        choice: FontChoice::Bundled,
                    },
                    cx,
                ))
                .child(select(
                    select_id,
                    Some(selected_value),
                    i18n::t("settings.font_system"),
                    options,
                    theme.palette(),
                    move |value, _window, cx| {
                        let choice = match value.as_ref() {
                            "bundled" => FontChoice::Bundled,
                            "system" => FontChoice::System,
                            value => family_choices_for_change
                                .iter()
                                .find(|(_, candidate)| candidate.as_ref() == value)
                                .map_or(FontChoice::System, |(family, _)| {
                                    FontChoice::Custom(family)
                                }),
                        };
                        ent.update(cx, |view, cx| view.set_font_choice(role, choice, cx));
                    },
                )),
        )
}

struct FontPillProps<'a> {
    theme: &'a Theme,
    ent: Entity<RootView>,
    role: FontRole,
    id: &'static str,
    label: &'static str,
    active: bool,
    hovered: Option<&'a Hover>,
    choice: FontChoice,
}

fn font_pill(props: FontPillProps<'_>, cx: &mut Context<RootView>) -> impl IntoElement {
    let FontPillProps {
        theme,
        ent,
        role,
        id,
        label,
        active,
        hovered,
        choice,
    } = props;
    pill(
        theme,
        id,
        label,
        active,
        hovered == Some(&Hover::Static(id)),
        move |_window, cx| {
            ent.update(cx, |view, cx| view.set_font_choice(role, choice, cx));
        },
        cx.listener(move |view, is_hovered: &bool, _window, cx| {
            view.set_hover(
                if *is_hovered {
                    Some(Hover::Static(id))
                } else {
                    None
                },
                cx,
            );
        }),
    )
}

pub(super) fn font_row(
    theme: &Theme,
    ent: Entity<RootView>,
    font_pref: FontPreference,
    availability: &FontAvailability,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(font_choice_row(
            FontChoiceRowProps {
                theme,
                ent: ent.clone(),
                role: FontRole::Ui,
                caption: i18n::t("settings.font_ui"),
                bundled_label: FONT_MISANS_VF,
                bundled_id: "font-ui-bundled",
                system_id: "font-ui-system",
                choice: font_pref.ui,
                availability,
                hovered,
            },
            cx,
        ))
        .child(font_choice_row(
            FontChoiceRowProps {
                theme,
                ent,
                role: FontRole::Mono,
                caption: i18n::t("settings.font_mono"),
                bundled_label: FONT_ROBOTO_MONO,
                bundled_id: "font-mono-bundled",
                system_id: "font-mono-system",
                choice: font_pref.mono,
                availability,
                hovered,
            },
            cx,
        ))
        .child(
            div()
                .text_size(tokens::FONT_CAPTION)
                .text_color(theme.fg_dim)
                .child(i18n::t("settings.font_hint").to_string()),
        )
        .child(
            div()
                .text_size(tokens::FONT_CAPTION)
                .text_color(theme.fg_dim)
                .child(effective_font_summary(theme)),
        )
}

pub(super) fn effective_font_summary(theme: &Theme) -> String {
    i18n::t("settings.font_effective")
        .replace("{ui}", theme.ui_font)
        .replace("{mono}", theme.mono_font)
}
