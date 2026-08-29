//! Settings row for desktop notification delivery (BN-07).

use std::collections::HashMap;

use gpui::{Context, Div, Entity, InteractiveElement, ParentElement, Styled, div};

use taskmanager_ui::inputs::select::{SelectOption, select};
use taskmanager_ui::inputs::switch::{Switch, SwitchState};

use crate::gpui_app::root::RootView;
use taskmanager_application::i18n;
use taskmanager_core::core::alerts::QuietBound;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

/// One switch row: "Desktop notifications" with the current policy state.
/// The switch updates the pure [`NotificationGate`] policy on `RootView`;
/// persistence happens through the regular Config save path.
pub(super) fn notify_row(
    t: &Theme,
    ent: Entity<RootView>,
    enabled: bool,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    let state = switches["desktop-notifications"].clone();
    state.update(cx, |state, cx| state.set_on(enabled, cx));
    div()
        .debug_selector(|| "desktop-notifications-row".to_string())
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_4)
                .child(
                    div()
                        .text_size(tokens::FONT_13)
                        .text_color(t.fg)
                        .child(i18n::t("settings.desktop_notifications")),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_11)
                        .text_color(t.fg_dim)
                        .child(i18n::t("settings.desktop_notifications_hint")),
                ),
        )
        .child(
            Switch::new(state, t.palette()).on_change(move |value, _win, cx| {
                ent.update(cx, |view, cx| view.set_notify_enabled(value, cx));
            }),
        )
}

/// One quiet-hours bound as a 0..=23 hour select (BN-07). Equal start/end
/// hours mean "no quiet hours" — the same semantics as the TUI/Iced pickers,
/// so a preference set in any frontend behaves identically.
fn quiet_hour_row(
    t: &Theme,
    ent: Entity<RootView>,
    bound: QuietBound,
    label: &'static str,
    current_hour: u8,
) -> impl gpui::IntoElement {
    let options: Vec<SelectOption> = (0..=23)
        .map(|hour| SelectOption::new(hour.to_string(), format!("{hour:02}:00")))
        .collect();
    select(
        match bound {
            QuietBound::Start => "quiet-hours-start",
            QuietBound::End => "quiet-hours-end",
        },
        Some(current_hour.to_string().into()),
        label,
        options,
        t.palette(),
        move |token, _win, cx| {
            let hour = token.parse::<u8>().unwrap_or(0);
            ent.update(cx, |view, cx| {
                view.set_quiet_hour_bound(bound, hour, cx);
            });
        },
    )
}

/// The quiet-hours section of the notifications group: start + end hour
/// selects. Both default to 00:00 (no quiet hours until changed).
pub(super) fn quiet_hours_rows(
    t: &Theme,
    ent: Entity<RootView>,
    start_hour: u8,
    end_hour: u8,
) -> Div {
    div()
        .debug_selector(|| "quiet-hours-rows".to_string())
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(quiet_hour_row(
            t,
            ent.clone(),
            QuietBound::Start,
            i18n::t("settings.quiet_hours_start"),
            start_hour,
        ))
        .child(quiet_hour_row(
            t,
            ent,
            QuietBound::End,
            i18n::t("settings.quiet_hours_end"),
            end_hour,
        ))
}
