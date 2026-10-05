//! Native navigation chrome; the rail owns one bounded scroll region.
use super::components::IcedElement;
use super::{alerts, current_window_capture_btn, page_icon, responsive::ChromePresentation};
use crate::{
    app::{FocusTarget, Message},
    focus,
    i18n::{self, Key},
};
use iced::Element;
use iced::widget::{column, row, scrollable};
use taskmanager_application::i18n::t;
use taskmanager_shell::{PageHelp, page_help};
use taskmanager_theme::tokens;
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::navigation::NavOrientation;
pub(super) enum NavigationChrome<'a> {
    Horizontal(IcedElement<'a>),
    Vertical {
        rail: IcedElement<'a>,
        toolbar: IcedElement<'a>,
    },
}
pub(super) fn render(app: &crate::IcedApp) -> NavigationChrome<'_> {
    let shell = &app.shell;
    let theme_snapshot = app.theme();
    let language = app.language();
    // One GPUI-shaped nav strip: the page tabs (accent-filled when active — the
    // same `choice_pill` the Performance device rail uses) on the left, a flex
    // space, then the toolbar triggers pinned right. Collapses the old three
    // plain rows (static title / text tabs / toolbar) into the single chrome bar
    // GPUI renders, so the active page reads at a glance. Page and primary
    // toolbar icons use the shared semantic SVG registry through Iced's own
    // `svg` widget.
    let current_page = shell.page();
    // The frontend-local alerts route suppresses the shared-tab highlight so
    // only one route reads as active at a time.
    let alerts_open = app.alerts_page_open();
    let mut page_tabs: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = page_help()
        .into_iter()
        .map(|PageHelp { page, label, .. }| {
            focus::choice_pill_with_icon(
                theme_snapshot,
                FocusTarget::PageTab(page),
                page_icon(page),
                if app.navigation_rail_compact() && app.nav_orientation == NavOrientation::Vertical
                {
                    String::new()
                } else {
                    label.to_string()
                },
                page == current_page && !alerts_open,
                Message::SelectPage(page),
            )
        })
        .collect();
    // The alerts page rides the same tab strip as the shared pages (an
    // Iced-local route outside the `AppPage` set).
    page_tabs.push(alerts::page_tab_pill(app));
    let toolbar_items: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = vec![
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::NavigationToggle,
            IconId::Sidebar,
            t("navigation.toggle"),
            Message::ToggleNavigation,
        ),
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::SettingsTrigger,
            IconId::Settings,
            i18n::t(language, Key::Settings),
            Message::OpenSettings,
        ),
        focus::ghost_button(
            theme_snapshot,
            FocusTarget::ContainersTrigger,
            i18n::t(language, Key::Containers),
            Message::OpenContainers,
        ),
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::HealthTrigger,
            IconId::Health,
            i18n::t(language, Key::Health),
            Message::OpenHealth,
        ),
        current_window_capture_btn(theme_snapshot, language),
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::Export,
            IconId::Export,
            i18n::t(language, Key::Export),
            Message::ExportSnapshot,
        ),
        focus::ghost_button_with_icon(
            theme_snapshot,
            FocusTarget::AboutTrigger,
            IconId::System,
            i18n::t(language, Key::About),
            Message::OpenAbout,
        ),
    ];
    // The full page vocabulary plus the five toolbar actions is wider than a
    // normal 1180px desktop viewport. Give the route strip its own horizontal
    // viewport and put actions on a second bounded row before they can paint
    // past the right edge. The wide 1440px+ layout keeps the original one-row
    // desktop composition. The 1320px single-row seam is the frame budget's
    // chrome presentation (responsive.rs), not a local literal.
    let chrome = ChromePresentation::for_width(app.viewport_width());
    let wrapped_chrome = app.compact_layout() || chrome.is_wrapped();
    let toolbar: Element<'_, Message, iced::Theme, iced::Renderer> = if app.compact_layout() {
        scrollable(row(toolbar_items).spacing(4))
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::default(),
            ))
            .height(iced::Length::Fixed(36.0))
            .width(iced::Length::Fill)
            .into()
    } else if wrapped_chrome {
        // Keep the action band to one bounded row. A wrapped action grid made
        // the 1180px capture spend a third row on About, pushing the actual
        // page body below the fold. Horizontal scrolling preserves every
        // action without changing the page's vertical budget.
        scrollable(row(toolbar_items).spacing(4))
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::default(),
            ))
            .height(iced::Length::Fixed(36.0))
            .width(iced::Length::Fill)
            .into()
    } else {
        row(toolbar_items).spacing(4).into()
    };
    // The wide layout keeps the action toolbar pinned to the trailing edge.
    // Compact windows get intentional rows: routes remain in one bounded
    // strip and actions get their own bounded horizontal row. Mixing both in
    // one horizontal scroller made the first screenshot look like controls
    // had disappeared behind the right edge even though they were reachable.
    if app.nav_orientation == NavOrientation::Vertical {
        let rail = scrollable(column(page_tabs).spacing(6))
            .width(iced::Length::Fixed(app.navigation_rail_width()))
            .height(iced::Length::Fill)
            .into();
        return NavigationChrome::Vertical { rail, toolbar };
    }
    let nav: Element<'_, Message, iced::Theme, iced::Renderer> = if wrapped_chrome {
        let page_nav = scrollable(
            row(page_tabs)
                .spacing(4)
                .padding([f32::from(tokens::SPACE_2), f32::from(tokens::SPACE_4)]),
        )
        .direction(iced::widget::scrollable::Direction::Horizontal(
            iced::widget::scrollable::Scrollbar::default(),
        ))
        .height(iced::Length::Fixed(44.0))
        .width(iced::Length::Fill);
        column![page_nav, toolbar]
            .spacing(4)
            .width(iced::Length::Fill)
            .into()
    } else {
        row(page_tabs)
            .spacing(4)
            .push(iced::widget::Space::new().width(iced::Length::Fill))
            .push(toolbar)
            .padding([f32::from(tokens::SPACE_2), f32::from(tokens::SPACE_4)])
            .into()
    };

    NavigationChrome::Horizontal(nav)
}
