//! Iced controls over the shared saved-view model.

use crate::app::{FocusTarget, Message};
use crate::focus;
use iced::widget::{container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_shell::saved_views::{SavedViewPreset, SavedViewTransferFeedback, feedback_text};
use taskmanager_shell::{ProcessStatusFilter, SortCol};
use taskmanager_theme::{Theme, tokens};

#[derive(Clone, Copy, Debug)]
pub struct PresetsRibbonState {
    pub filter: ProcessStatusFilter,
    pub sort: SortCol,
    pub ascending: bool,
    pub feedback: Option<SavedViewTransferFeedback>,
    pub compact: bool,
}

/// Render the Presets Ribbon bar above the process table.
pub fn presets_ribbon<'a>(
    theme_snapshot: &'a Theme,
    presets: &'a [SavedViewPreset],
    state: PresetsRibbonState,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let mut items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> =
        vec![focus::ghost_button(
            theme_snapshot,
            FocusTarget::SavedViewsTrigger,
            t("saved_views.title"),
            Message::OpenSavedViews,
        )];

    for preset in presets {
        let is_active = preset.filter == state.filter
            && preset.sort_col == state.sort
            && preset.sort_asc == state.ascending;

        let btn = focus::choice_pill(
            theme_snapshot,
            FocusTarget::SavedViewPreset(preset.id),
            preset.display_name(),
            is_active,
            Message::ApplySavedView(preset.id),
        );
        items.push(btn);
    }

    // Save Current, Export, Import action buttons
    let save_btn = focus::dynamic_button(
        theme_snapshot,
        FocusTarget::SavedViewSaveCurrent,
        t("saved_views.save_current").to_string(),
        Message::SaveCurrentProcessView,
        false,
    );
    let export_btn = focus::dynamic_button(
        theme_snapshot,
        FocusTarget::SavedViewExport,
        t("common.export").to_string(),
        Message::ExportSavedViews,
        false,
    );
    let import_btn = focus::dynamic_button(
        theme_snapshot,
        FocusTarget::SavedViewImport,
        t("common.import").to_string(),
        Message::ImportSavedViews,
        false,
    );

    items.extend([save_btn, export_btn, import_btn]);

    if let Some(fb) = state.feedback {
        let msg = feedback_text(fb);
        let is_error = matches!(
            fb,
            SavedViewTransferFeedback::ExportFailed
                | SavedViewTransferFeedback::ClipboardEmpty
                | SavedViewTransferFeedback::ImportInvalid
        );
        let color = if is_error {
            crate::theme_binding::color(theme_snapshot.palette().danger)
        } else {
            crate::theme_binding::color(theme_snapshot.palette().accent)
        };
        items.push(
            text(msg)
                .size(f32::from(tokens::FONT_11))
                .color(color)
                .into(),
        );
    }

    if state.compact {
        // Keep the preset ribbon to a single bounded horizontal scrollable strip
        // so it never wraps into multiple vertical rows that consume the table viewport.
        iced::widget::scrollable(row(items).spacing(6).align_y(iced::Alignment::Center))
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::default(),
            ))
            .height(Length::Fixed(32.0))
            .width(Length::Fill)
            .into()
    } else {
        container(row(items).spacing(6).align_y(iced::Alignment::Center))
            .padding([4, 8])
            .width(Length::Fill)
            .into()
    }
}
