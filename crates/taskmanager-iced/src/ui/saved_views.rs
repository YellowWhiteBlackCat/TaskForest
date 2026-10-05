//! Independent saved-view review with fixed transfers around complete, scrollable rows.

use super::components::IcedElement;
use super::overlays::bounded_modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::focus;
use iced::Length;
use iced::widget::{column, row, scrollable, text};
use taskmanager_application::i18n::t;
use taskmanager_shell::saved_views::{feedback_text, review_rows};

pub(super) fn render(app: &crate::IcedApp) -> IcedElement<'_> {
    let mut body = column![text(t("saved_views.help"))]
        .spacing(12)
        .width(Length::Fill);
    for preset in review_rows(&app.saved_views) {
        let mut actions = row![focus::dynamic_button(
            app.theme(),
            FocusTarget::SavedViewPreset(preset.id),
            t("common.apply").into(),
            Message::ApplySavedView(preset.id),
            false
        )]
        .spacing(6);
        if preset.is_user_saved() {
            actions = actions.push(focus::dynamic_button(
                app.theme(),
                FocusTarget::SavedViewRemove(preset.id),
                t("common.remove").into(),
                Message::DeleteSavedView(preset.id),
                false,
            ));
        }
        body = body.push(
            column![
                text(preset.display_name()),
                text(format!(
                    "{} · {} · {}",
                    preset.filter.label(),
                    preset.sort_col.label(),
                    if preset.sort_asc { "↑" } else { "↓" }
                )),
                actions.wrap()
            ]
            .spacing(4),
        );
    }
    if let Some(feedback) = app.saved_view_feedback {
        body = body.push(text(feedback_text(feedback)));
    }
    let actions = vec![
        focus::dynamic_button(
            app.theme(),
            FocusTarget::SavedViewSaveCurrent,
            t("saved_views.save_current").into(),
            Message::SaveCurrentProcessView,
            false,
        ),
        focus::dynamic_button(
            app.theme(),
            FocusTarget::SavedViewExport,
            t("common.export").into(),
            Message::ExportSavedViews,
            false,
        ),
        focus::dynamic_button(
            app.theme(),
            FocusTarget::SavedViewImport,
            t("common.import").into(),
            Message::ImportSavedViews,
            false,
        ),
    ];
    bounded_modal_overlay(
        app,
        t("saved_views.title"),
        scrollable(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        actions,
    )
}
