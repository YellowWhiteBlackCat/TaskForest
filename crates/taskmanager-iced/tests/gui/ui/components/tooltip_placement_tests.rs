// test-intent: behavior
//! Measured placement of the production tooltip overlays. Inventory-row
//! details must not cover their own dense row; ordinary focus hints keep
//! their compact below-the-trigger placement.

use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::Tree;
use iced::mouse;
use iced::{Element, Point, Rectangle, Size, Vector};
use taskmanager_assets::embedded_fonts;
use taskmanager_theme::{Theme, tokens};

type TestElement<'a> = Element<'a, crate::app::Message, iced::Theme, iced::Renderer>;

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

fn mounts_tooltip(
    element: &mut TestElement<'_>,
    renderer: &iced::Renderer,
    viewport: Size,
    limits: Size,
) -> Node {
    let mut tree = Tree::new(&*element);
    let node =
        element
            .as_widget_mut()
            .layout(&mut tree, renderer, &Limits::new(Size::ZERO, limits));
    let overlay = element
        .as_widget_mut()
        .overlay(
            &mut tree,
            iced::advanced::layout::Layout::new(&node),
            renderer,
            &Rectangle::with_size(viewport),
            Vector::ZERO,
        )
        .expect("the capture tooltip mounts through the overlay channel");
    let mut overlay = iced::advanced::overlay::Nested::new(overlay);
    let node = overlay.layout(renderer, viewport);
    node.children()
        .first()
        .and_then(|outer| outer.children().first())
        .cloned()
        .expect("the mounted tooltip owns one floating-panel node")
}

fn dispatch(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    event: iced::Event,
    cursor: mouse::Cursor,
) {
    let mut messages = Vec::new();
    {
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        element.as_widget_mut().update(
            tree,
            &event,
            Layout::new(node),
            cursor,
            renderer,
            &mut clipboard,
            &mut shell,
            viewport,
        );
    }
}

fn row_detail_tooltip(theme: Theme) -> TestElement<'static> {
    let marker = crate::test_support::temp_dir("tooltip-placement").join("markers.log");
    let content = iced::widget::container(iced::widget::text("38%"))
        .width(iced::Length::Fixed(90.0))
        .height(iced::Length::Fixed(32.0))
        .into();
    crate::ui::components::tooltip_owned_for_capture(
        theme,
        content,
        "6/7 current · partial: Permission denied".to_owned(),
        marker,
    )
}

#[test]
fn inventory_row_detail_uses_the_far_viewport_edge_in_standard_and_compact_windows() {
    let renderer = renderer();
    let theme = Theme::dark();
    for (viewport, limits) in [
        (Size::new(1180.0, 780.0), Size::new(1180.0, 780.0)),
        (Size::new(720.0, 480.0), Size::new(720.0, 480.0)),
    ] {
        let mut element = row_detail_tooltip(theme);
        let overlay_node = mounts_tooltip(&mut element, &renderer, viewport, limits);
        let tooltip = overlay_node.bounds();
        let trigger = Rectangle::new(Point::ORIGIN, Size::new(90.0, 32.0));
        let margin = f32::from(tokens::SPACE_8);
        assert!(
            tooltip.y >= margin - 0.01
                && tooltip.y + tooltip.height <= viewport.height - margin + 0.01
                && tooltip.x >= margin - 0.01
                && tooltip.x + tooltip.width <= viewport.width - margin + 0.01,
            "row tooltip stays inside the {viewport:?} window: {tooltip:?}"
        );
        assert!(
            !tooltip.intersects(&trigger),
            "row tooltip preserves the hovered row: {tooltip:?}"
        );
        assert!(
            (tooltip.y + tooltip.height + margin - viewport.height).abs() < 0.01,
            "upper-region row tooltip parks at the lower edge: {tooltip:?}"
        );
    }
}

#[test]
fn lower_inventory_row_detail_uses_the_upper_viewport_edge() {
    let renderer = renderer();
    let viewport = Size::new(1180.0, 780.0);
    let mut element = row_detail_tooltip(Theme::dark());
    let mut tree = Tree::new(&element);
    let node =
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, viewport));
    let translated = node.clone().translate(Vector::new(200.0, 700.0));
    let overlay = element
        .as_widget_mut()
        .overlay(
            &mut tree,
            iced::advanced::layout::Layout::new(&node),
            &renderer,
            &Rectangle::with_size(viewport),
            Vector::new(200.0, 700.0),
        )
        .expect("the lower row tooltip mounts");
    let mut overlay = iced::advanced::overlay::Nested::new(overlay);
    let overlay_node = overlay.layout(&renderer, viewport);
    let tooltip = overlay_node.children()[0].children()[0].bounds();
    assert!(
        (tooltip.y - f32::from(tokens::SPACE_8)).abs() < 0.01,
        "lower-region row tooltip parks at the upper edge: {tooltip:?}, trigger={translated:?}"
    );
}

#[test]
fn ordinary_focus_hint_keeps_its_below_trigger_placement() {
    let renderer = renderer();
    let theme = Theme::dark().with_focus_visible(true);
    let viewport = Size::new(1180.0, 780.0);
    let content: TestElement<'_> = iced::widget::container(iced::widget::text("trigger"))
        .width(iced::Length::Fixed(100.0))
        .height(iced::Length::Fixed(32.0))
        .into();
    let mut element: TestElement<'_> =
        crate::ui::components::tooltip(&theme, content, "a focus hint");
    let mut tree = Tree::new(&element);
    let node =
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, viewport));
    let window = Rectangle::with_size(viewport);
    let pointer = Point::new(10.0, 10.0);
    dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &window,
        iced::Event::Mouse(mouse::Event::CursorMoved { position: pointer }),
        mouse::Cursor::Available(pointer),
    );
    let overlay = element
        .as_widget_mut()
        .overlay(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &window,
            Vector::ZERO,
        )
        .expect("pointer hover opens the ordinary hint");
    let mut overlay = iced::advanced::overlay::Nested::new(overlay);
    let hint = overlay.layout(&renderer, viewport).children()[0].children()[0].bounds();
    assert!(
        hint.y >= node.bounds().height && hint.y + hint.height <= viewport.height,
        "an ordinary hint stays below its trigger: {hint:?}"
    );
}
