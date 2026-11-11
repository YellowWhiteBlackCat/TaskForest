use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{Id, Tree};
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::{Element, Event, Rectangle, Size, mouse};
use taskmanager_application::AppPage;
use taskmanager_assets::embedded_fonts;
use taskmanager_theme::Theme;

use crate::app::{FocusTarget, Message, ServiceStatusFilter};

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

fn focus_operation(
    element: &mut crate::ui::components::IcedElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
) -> bool {
    #[derive(Default)]
    struct FocusSegment {
        focused: bool,
    }

    impl Operation<()> for FocusSegment {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
            visit(self);
        }

        fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
            if id
                == Some(&Id::from(crate::focus::focus_id(
                    FocusTarget::ServiceStatusFilterSegment,
                )))
            {
                state.focus();
                self.focused = state.is_focused();
            }
        }
    }

    let mut operation = FocusSegment::default();
    element
        .as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation.focused
}

fn arrow_right() -> Event {
    let key = Key::Named(Named::ArrowRight);
    Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(Code::ArrowRight),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    })
}

fn segmented_element<'a>(theme: &'a Theme) -> TestElement<'a> {
    let choices = ServiceStatusFilter::ALL
        .iter()
        .map(|filter| (filter.label().to_owned(), filter.index()))
        .collect::<Vec<_>>();
    crate::ui::components::segmented(
        theme,
        FocusTarget::ServiceStatusFilterSegment,
        &choices,
        ServiceStatusFilter::All.index(),
        crate::app::services_state::service_filter_message,
    )
}

#[test]
fn services_status_segment_keyboard_step_applies_the_typed_filter() {
    let theme = *crate::IcedApp::demo().theme();
    let renderer = renderer();
    let size = Size::new(420.0, 60.0);
    let viewport = Rectangle::with_size(size);
    let mut element = segmented_element(&theme);
    let mut tree = Tree::new(&element);
    let node = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
    assert!(focus_operation(&mut element, &mut tree, &node, &renderer));

    let mut messages = Vec::new();
    {
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        element.as_widget_mut().update(
            &mut tree,
            &arrow_right(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
    }
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::SelectServiceStatusFilter(ServiceStatusFilter::Active)
        )),
        "arrow right must publish the next typed status filter"
    );

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let prefiltered = app.services_projection("").1.len();
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(prefiltered, 5, "the baseline projection starts unfiltered");
    assert_eq!(app.services_status_filter, ServiceStatusFilter::Active);
    assert_eq!(
        app.services_projection("").1.len(),
        3,
        "cached visible projection must include the new status key"
    );
}
