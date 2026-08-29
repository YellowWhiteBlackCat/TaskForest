//! Reusable Iced presentation components.
//!
//! This module owns renderer geometry and visual grammar that is shared by
//! several Iced pages. Page modules keep their projections and interaction
//! decisions; they compose these components instead of rebuilding the same
//! panel, state, key/value, or outer-page structure.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_application::{RefreshRequest, SourceNotice, SourceStateKind, merge_source_lines};
use taskmanager_core::core::source::SourceStatus;

use taskmanager_shell::presentation::control_error_detail;
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui_contract::IconId;

use crate::app::{FocusTarget, Message};
use crate::theme;

pub(crate) mod highlight;
pub(crate) mod inputs;
pub(crate) mod popover;
pub(crate) mod primitives;
pub(crate) mod selectable_text;

pub(crate) use inputs::*;
pub(crate) use popover::Popover;
pub(crate) use primitives::*;
pub(crate) use selectable_text::SelectableText;

pub(crate) type IcedElement<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

/// The ordinary full-width page shell used by the root view.
#[must_use]
pub(crate) fn page_scaffold<'a>(
    navigation: IcedElement<'a>,
    body: IcedElement<'a>,
    footer: IcedElement<'a>,
) -> IcedElement<'a> {
    column![navigation, body, footer]
        .spacing(8)
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The card surface every card-shaped iced component wears: the theme's
/// elevated panel fill with the palette's border and panel radius plus the
/// quiet card shadow. Exposed as the components module's owned seam so pages
/// compose card surfaces through the component layer instead of reaching
/// into the style registry directly.
#[must_use]
pub(crate) fn card_surface(theme_snapshot: &Theme) -> iced::widget::container::Style {
    theme::card_style(theme_snapshot)
}

/// A titled card for page-local facts and compact detail groups. Geometry is
/// token-bound: SPACE_* padding and title/body gutter, the body role's type
/// size for the title, and the palette's panel radius through
/// [`card_surface`].
#[must_use]
pub(crate) fn titled_card<'a>(
    theme_snapshot: &'a Theme,
    title: &'static str,
    body: impl Into<IcedElement<'a>>,
) -> IcedElement<'a> {
    container(
        column![text(title).size(f32::from(tokens::FONT_BODY)), body.into()]
            .spacing(f32::from(tokens::SPACE_6)),
    )
    .style(move |_| card_surface(theme_snapshot))
    .padding(f32::from(tokens::SPACE_10))
    .width(Length::Fill)
    .into()
}

/// A centered, neutral state panel for loading, empty, and collecting states.
/// The caller owns the localized message; this component owns the geometry and
/// the visual state grammar. A thin wrapper over [`state_panel`]: the shared
/// empty-state grammar (Applications, the GPUI empty-state default icon) with
/// no detail and no action.
#[must_use]
pub(crate) fn message_panel<'a>(
    theme_snapshot: &'a Theme,
    message: &'static str,
) -> IcedElement<'a> {
    state_panel(
        theme_snapshot,
        PanelState::Empty,
        IconId::Applications,
        message,
        None,
        None,
    )
}

/// Render a source failure without turning it into a successful-looking empty
/// result. The retry control is page-scoped and remains keyboard reachable.
/// A thin wrapper over [`state_panel`]: the source semantics map at this seam
/// ([`source_panel_state`]) and the merged failure detail plus retry action
/// become the panel's detail/action slots.
#[must_use]
pub(crate) fn source_state_panel<'a>(
    theme_snapshot: &'a Theme,
    sources: Option<&[SourceStatus]>,
    request: RefreshRequest,
) -> Option<IcedElement<'a>> {
    let merged = merge_source_lines(sources?)?;
    let title = t(banner_title_key(merged.kind));
    let reason = control_error_detail(merged.notice.failure());
    let action = source_notice_action(theme_snapshot, merged.notice, request);
    Some(state_panel(
        theme_snapshot,
        source_panel_state(merged.kind),
        IconId::TriangleAlert,
        title,
        Some(reason.to_string()),
        Some(action),
    ))
}

/// The source-state → panel-state seam the [`source_state_panel`] wrapper
/// renders through: a degraded source still serves some rows (partial —
/// caution), every other merged failure line means the source did not answer
/// (unavailable — failure). Typed as its own function so the headless tests
/// pin the mapping.
#[must_use]
pub(crate) fn source_panel_state(kind: SourceStateKind) -> PanelState {
    if kind == SourceStateKind::Degraded {
        PanelState::Partial
    } else {
        PanelState::Unavailable
    }
}

/// Compact warning placed above usable rows. It carries the same source and
/// retry projection as [`source_state_panel`] while preserving the table.
#[must_use]
pub(crate) fn source_notice_banner<'a>(
    theme_snapshot: &'a Theme,
    sources: Option<&[SourceStatus]>,
    request: RefreshRequest,
) -> Option<IcedElement<'a>> {
    let merged = merge_source_lines(sources?)?;
    let title = t(banner_title_key(merged.kind));
    let reason = control_error_detail(merged.notice.failure());
    let action = source_notice_action(theme_snapshot, merged.notice, request);
    Some(
        container(
            row![
                text("⚠")
                    .size(f32::from(tokens::FONT_16))
                    .color(taskmanager_theme::iced::color(
                        theme_snapshot.palette().warning
                    )),
                column![
                    text(title).size(f32::from(tokens::FONT_12)),
                    text(reason).size(f32::from(tokens::FONT_11))
                ]
                .spacing(2),
                action,
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .style(move |_| source_panel_style(theme_snapshot))
        .padding([6, 8])
        .width(Length::Fill)
        .into(),
    )
}

pub(super) fn banner_title_key(kind: SourceStateKind) -> &'static str {
    if kind == SourceStateKind::Degraded {
        "source.partial_title"
    } else {
        "source.unavailable_title"
    }
}

fn source_notice_action<'a>(
    theme_snapshot: &'a Theme,
    notice: SourceNotice,
    request: RefreshRequest,
) -> IcedElement<'a> {
    if notice.is_retryable() {
        crate::focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::SourceRetry(request),
            taskmanager_ui_contract::IconId::Refresh,
            t("common.refresh"),
            Message::RefreshSource(request),
        )
    } else {
        text(t("source.retry_after_change"))
            .size(f32::from(tokens::FONT_11))
            .into()
    }
}

fn source_panel_style(theme_snapshot: &Theme) -> iced::widget::container::Style {
    let mut style = theme::panel_style(theme_snapshot);
    style.border.color = taskmanager_theme::iced::color(theme_snapshot.palette().warning);
    style
}

/// Render a stable label/value stack from owned display strings.
///
/// Values are already folded by the caller's view model, so this component
/// never invents fallback data or interprets provider state.
#[must_use]
pub(crate) fn key_value_rows(rows: Vec<(String, String)>) -> IcedElement<'static> {
    column(
        rows.into_iter()
            .map(|(label, value)| {
                row![
                    text(label).width(Length::Fixed(180.0)),
                    text(value).width(Length::Fill),
                ]
                .spacing(8)
                .padding(4)
                .width(Length::Fill)
                .into()
            })
            .collect::<Vec<IcedElement<'static>>>(),
    )
    .spacing(2)
    .into()
}
