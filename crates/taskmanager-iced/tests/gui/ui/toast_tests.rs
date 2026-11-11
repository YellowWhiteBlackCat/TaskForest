// test-intent: behavior

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{Id, Tree};
use iced::{Element, Rectangle, Size};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use crate::Message;
use crate::app::FocusTarget;
use crate::focus::focus_id;

type TestElement<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

#[derive(Default)]
struct CardProbe {
    text: Vec<(String, Rectangle)>,
    focus: Vec<(Id, Rectangle)>,
}

impl Operation<()> for CardProbe {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, fragment: &str) {
        self.text.push((fragment.to_owned(), bounds));
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        if let Some(id) = id {
            self.focus.push((id.clone(), bounds));
        }
    }
}

fn theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
}

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

fn probe(element: &mut TestElement<'_>, viewport: Size) -> CardProbe {
    let renderer = renderer();
    let mut tree = Tree::new(&*element);
    let node =
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, viewport));
    let mut probe = CardProbe::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut probe);
    probe
}

fn notice_shell(text: &str) -> ShellApp {
    let mut shell = ShellApp::new();
    shell.report_notice(
        FeedbackSource::Interaction,
        FeedbackSeverity::Success,
        FeedbackLifecycle::UntilReplaced,
        text,
    );
    shell
}

#[test]
fn the_toast_mounts_the_shared_notice_and_a_keyboard_reachable_dismiss_stop() {
    let theme = theme();
    let shell = notice_shell("Copied to clipboard");
    let notice = shell
        .feedback_presentation()
        .expect("a feedback notice is mounted");
    let mut element = crate::ui::components::toast(&theme, notice, 720.0);
    let probe = probe(&mut element, Size::new(720.0, 480.0));
    assert!(
        probe
            .text
            .iter()
            .any(|(text, _)| text == "Copied to clipboard"),
        "the toast paints the shell-owned notice text"
    );
    let dismiss_id = Id::from(focus_id(FocusTarget::ToastDismiss));
    assert!(
        probe.focus.iter().any(|(id, bounds)| *id == dismiss_id
            && bounds.x >= 0.0
            && bounds.x + bounds.width <= 720.0),
        "the dismiss action is a measured keyboard focus stop inside the viewport"
    );
}

#[test]
fn the_toast_card_stays_inside_a_compact_720_by_480_frame() {
    let theme = theme();
    let message = "Long capture path: /run/media/workspace/target/capture/runs/receipt.png";
    let shell = notice_shell(message);
    let notice = shell
        .feedback_presentation()
        .expect("a feedback notice is mounted");
    for width in [320.0, 720.0, 1180.0] {
        let mut element = crate::ui::components::toast(&theme, notice, width);
        let viewport = Size::new(width, 480.0);
        let renderer = renderer();
        let mut tree = Tree::new(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &Limits::new(Size::ZERO, viewport),
        );
        assert!(
            node.bounds().x >= 0.0
                && node.bounds().y >= 0.0
                && node.bounds().x + node.bounds().width <= width
                && node.bounds().y + node.bounds().height <= viewport.height,
            "the toast layer stays inside the {width}x480 frame: {:?}",
            node.bounds()
        );
    }
}
