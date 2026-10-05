use super::*;
use crate::app::{FocusTarget, Message};
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{
    Tree,
    operation::{black_box, focusable},
};
use iced::{Pixels, Size};
use taskmanager_assets::embedded_fonts;

#[test]
fn settings_keyboard_focus_reveals_the_actual_switch_in_every_allocated_viewport() {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("fonts");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("renderer");
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 360.0),
        (480.0, 960.0),
        (1920.0, 1080.0),
    ] {
        for section in ["hc", "zero-values"] {
            let mut app = crate::IcedApp::demo();
            let _ = app.update(Message::OpenSettings);
            let _ = app.update(Message::WindowResized(Size::new(width, height)));
            let mut element = crate::ui::view(&app);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &Limits::new(Size::ZERO, Size::new(width, height)),
            );
            let id = Id::from(crate::focus::focus_id(FocusTarget::SettingsChoice {
                section,
                index: 0,
            }));
            let mut focus = focusable::focus::<bool>(id.clone());
            element.as_widget_mut().operate(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &mut black_box(&mut focus),
            );
            let mut operation: Box<dyn Operation<bool>> = Box::new(Measure {
                target: Some(id),
                scroll: true,
                ..Measure::default()
            });
            let mut result = None;
            for _ in 0..3 {
                element.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &mut black_box(operation.as_mut()),
                );
                match operation.finish() {
                    Outcome::Some(visible) => {
                        result = Some(visible);
                        break;
                    }
                    Outcome::Chain(next) => operation = next,
                    Outcome::None => break,
                }
            }
            assert_eq!(result, Some(true), "{section} at {width}x{height}");
            let mut missing = Measure {
                target: Some(Id::from("foreign-control")),
                ..Measure::default()
            };
            element.as_widget_mut().operate(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &mut black_box(&mut missing),
            );
            assert!(
                matches!(missing.finish(), Outcome::Some(false)),
                "a foreign control must not certify the scene"
            );
        }
    }
}
