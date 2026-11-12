// test-intent: behavior
//
// The Applications action bar runs the priority preset on the SHARED select
// component: the page mounts the shared focus stop, the trigger keeps the
// localized caption and placeholder, and the options carry the shared
// priority-tier vocabulary.

use super::*;
use crate::app::IcedApp;
use crate::text_metrics::measured_text_width;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::overlay::Nested;
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Element, Rectangle, Size, Vector};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::presentation::priority_tier_label;
use taskmanager_theme::tokens::{FONT_CAPTION, SPACE_8};
use taskmanager_theme::{
    GlassSurfaceRole, GlassTier, HighContrast, LightDark, ResolvedFonts, Skin, Theme,
};
use taskmanager_ui_contract::presentation::sizes::ControlRole;

type TestElement<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

fn renderer() -> iced::Renderer {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer")
}

#[test]
fn applications_empty_body_spends_the_inset_page_header_surface() {
    assert_eq!(GlassSurfaceRole::PageHeader.tier(), GlassTier::Card);
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            for high_contrast in [HighContrast::Off, HighContrast::On] {
                let theme =
                    Theme::build(skin, mode, high_contrast, ResolvedFonts::system_for(skin));
                let inset = empty_table_body_style(&theme);
                let floating = theme::panel_style(&theme);
                assert_eq!(
                    inset.background, floating.background,
                    "the inset role keeps the shared Card glass fill"
                );
                assert_eq!(
                    inset.border, floating.border,
                    "the inset role keeps the shared Card hairline and radius"
                );
                assert!(
                    floating.shadow.color.a > 0.0,
                    "the floating panel style must still carry its shadow for this contrast"
                );
                assert_eq!(
                    inset.shadow,
                    iced::Shadow::default(),
                    "the empty Applications body must not stack a floating elevation shadow"
                );
            }
        }
    }
}

#[derive(Default)]
struct PageWidgets {
    text: Vec<(String, Rectangle)>,
    focus_ids: Vec<Id>,
    focus_bounds: Vec<(Id, Rectangle)>,
    scroll_viewports: Vec<(Rectangle, Rectangle)>,
}

impl Operation for PageWidgets {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, fragment: &str) {
        self.text.push((fragment.to_owned(), bounds));
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        if let Some(id) = id {
            self.focus_ids.push(id.clone());
            self.focus_bounds.push((id.clone(), bounds));
        }
    }

    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        _translation: iced::Vector,
        _state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        self.scroll_viewports.push((bounds, content_bounds));
    }
}

/// The measured rectangle of the shared priority-select focus stop.
fn priority_select_bounds(page: &PageWidgets) -> Rectangle {
    let expected = Id::from(crate::focus::focus_id(FocusTarget::ProcessPrioritySelect));
    let (_, bounds) = page
        .focus_bounds
        .iter()
        .find(|(id, _)| *id == expected)
        .expect("the priority preset must register a measured focus stop");
    *bounds
}

/// One Applications page: the base tree and the mounted overlay tree.
///
/// The priority preset lives in the anchored Actions overflow panel, so the
/// select is mounted in the OVERLAY layer — the same layer the column
/// chooser and every other anchored panel uses. Measuring only the base
/// tree would prove the panel is absent, not that the control is broken.
struct PageTrees {
    base: PageWidgets,
    overlay: PageWidgets,
}

fn render_page(size: Size) -> PageTrees {
    let mut app = IcedApp::demo();
    // The priority preset lives in the anchored Actions overflow panel in
    // the current action-bar layout; open that typed surface so the test
    // measures the mounted select instead of a stale toolbar position.
    let _ = app.update(Message::OpenProcessActionsMenu);
    let renderer = renderer();
    let mut element: TestElement<'_> = applications_page(&app);
    let mut tree = Tree::new(&element);
    let node: Node =
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
    let mut base = PageWidgets::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut base);
    let mut overlay = PageWidgets::default();
    if let Some(mounted) = element.as_widget_mut().overlay(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &Rectangle::with_size(size),
        Vector::ZERO,
    ) {
        let mut nested = Nested::new(mounted);
        let overlay_node = nested.layout(&renderer, size);
        nested.operate(Layout::new(&overlay_node), &renderer, &mut overlay);
    }
    PageTrees { base, overlay }
}

/// The priority preset rides the shared select: the page registers the
/// shared focus stop and shows the localized priority caption directly
/// beside the trigger instead of a bare toolkit picker.
#[test]
fn the_priority_preset_rides_the_shared_select_component() {
    let page = render_page(Size::new(1180.0, 780.0));

    assert!(
        page.overlay.focus_ids.contains(&priority_id()),
        "the priority preset must be a real focus stop on the shared select"
    );
    let bounds = priority_select_bounds(&page.overlay);
    let theme = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    let extent = crate::theme::control_extent(&theme, ControlRole::Input);
    assert!(
        bounds.width > 0.0 && bounds.height > 0.0,
        "the shared select must own a real, visible rectangle: {bounds:?}"
    );
    assert!(
        page.base
            .scroll_viewports
            .iter()
            .any(|(viewport, _)| viewport.height >= extent - 0.001),
        "the action strip must own the shared ControlRole extent: {extent} vs {:?}",
        page.base.scroll_viewports
    );
    let priority_group_width = priority_group_width(&theme);
    assert!(
        bounds.width >= priority_group_width - caption_width(&theme) - 0.001,
        "the priority trigger must keep its measured caption+placeholder slot: {bounds:?} vs {priority_group_width}"
    );
    assert!(
        page.base
            .scroll_viewports
            .iter()
            .any(|(viewport, content)| content.width > viewport.width
                && content.width >= bounds.x + bounds.width - viewport.x),
        "the full action vocabulary must own a real horizontal scroll viewport: {:?}",
        page.base.scroll_viewports
    );

    let rendered: Vec<&str> = page
        .overlay
        .text
        .iter()
        .map(|(fragment, _)| fragment.as_str())
        .collect();
    assert!(
        rendered
            .iter()
            .any(|fragment| fragment.contains("Priority")),
        "the overflow panel must show the localized priority caption: {rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .any(|fragment| fragment.contains("Delete confirms")),
        "the shared action hint must remain visible in the panel: {rendered:?}"
    );

    // The options carry the shared priority-tier vocabulary, so the same
    // labels the GPUI action menu publishes are the ones Iced offers.
    let options = PriorityOption::all();
    let labels: Vec<String> = options.iter().map(|option| option.to_string()).collect();
    assert_eq!(labels.len(), options.len());
    assert!(
        labels.iter().all(|label| !label.is_empty()),
        "every priority option must resolve a shared label: {labels:?}"
    );
    for (option, label) in options.iter().zip(labels.iter()) {
        assert_eq!(
            *label,
            priority_tier_label(option.tier),
            "each option label resolves through the shell's one wording fold"
        );
    }
}

/// The compact frame keeps the same shared focus stop: the control stays
/// reachable and measurable inside the narrow viewport.
#[test]
fn the_priority_preset_stays_reachable_in_the_compact_frame() {
    let page = render_page(Size::new(720.0, 480.0));
    assert!(
        page.overlay.focus_ids.contains(&priority_id()),
        "the compact frame must keep the shared priority focus stop"
    );
    let bounds = priority_select_bounds(&page.overlay);
    assert!(
        bounds.width > 0.0 && bounds.height > 0.0,
        "the compact frame must keep the shared select measurable: {bounds:?}"
    );
    assert!(
        page.base
            .scroll_viewports
            .iter()
            .any(|(viewport, content)| content.width > viewport.width),
        "the compact frame must keep the action strip horizontally scrollable: {:?}",
        page.base.scroll_viewports
    );
}

fn priority_id() -> Id {
    Id::from(crate::focus::focus_id(FocusTarget::ProcessPrioritySelect))
}

fn caption_width(theme: &Theme) -> f32 {
    measured_text_width(t("proc.priority"), f32::from(FONT_CAPTION))
        + f32::from(SPACE_8)
        + crate::theme::control_padding_x(theme, ControlRole::Input)
}
