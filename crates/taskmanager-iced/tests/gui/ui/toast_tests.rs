// test-intent: behavior

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{Id, Tree};
use iced::{Element, Rectangle, Size};
use taskmanager_application::{AppAction, AppPage};
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
    let mut element = super::toast(&theme, notice, 720.0);
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
    let message = "Long capture receipt: workspace/target/capture/runs/receipt.png";
    let shell = notice_shell(message);
    let notice = shell
        .feedback_presentation()
        .expect("a feedback notice is mounted");
    for width in [320.0, 720.0, 1180.0] {
        let mut element = super::toast(&theme, notice, width);
        let viewport = Size::new(width, 480.0);
        let renderer = renderer();
        let mut tree = Tree::new(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &Limits::new(Size::ZERO, viewport),
        );
        let mut visual_probe = CardProbe::default();
        element.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut visual_probe,
        );
        let text_bounds = visual_probe
            .text
            .iter()
            .find(|(text, _)| text == message)
            .map(|(_, bounds)| *bounds)
            .expect("the long notice remains rendered");
        let dismiss_bounds = focus_bounds(&visual_probe, FocusTarget::ToastDismiss)
            .expect("the dismiss action remains keyboard reachable");
        assert!(
            node.bounds().x >= 0.0
                && node.bounds().y >= 0.0
                && node.bounds().x + node.bounds().width <= width
                && node.bounds().y + node.bounds().height <= viewport.height,
            "the toast layer stays inside the {width}x480 frame: {:?}",
            node.bounds()
        );
        assert!(
            text_bounds.x >= node.bounds().x
                && text_bounds.y >= node.bounds().y
                && text_bounds.x + text_bounds.width <= node.bounds().x + node.bounds().width
                && text_bounds.y + text_bounds.height <= node.bounds().y + node.bounds().height,
            "wrapped notice text stays inside the {width}px toast card: {text_bounds:?}"
        );
        assert!(
            dismiss_bounds.x >= node.bounds().x
                && dismiss_bounds.y >= node.bounds().y
                && dismiss_bounds.x + dismiss_bounds.width <= node.bounds().x + node.bounds().width
                && dismiss_bounds.y + dismiss_bounds.height
                    <= node.bounds().y + node.bounds().height,
            "the dismiss action stays visible inside the {width}px toast card: {dismiss_bounds:?}"
        );
    }
}

#[test]
fn the_root_toast_sits_below_navigation_and_stays_inside_the_viewport() {
    for viewport in [
        Size::new(1180.0, 780.0),
        Size::new(720.0, 480.0),
        Size::new(1440.0, 900.0),
    ] {
        let mut app = crate::IcedApp::demo();
        app.shell.set_feedback_activity("");
        app.shell.report_notice(
            FeedbackSource::Interaction,
            FeedbackSeverity::Success,
            FeedbackLifecycle::UntilReplaced,
            "Toast placement",
        );
        let _ = app.update(Message::WindowResized(viewport));
        let mut element = crate::ui::view_root(&app);
        let probe = probe(&mut element, viewport);

        let page_tab = focus_bounds(&probe, FocusTarget::PageTab(AppPage::Performance))
            .expect("the page navigation tab is rendered");
        let navigation_toggle = focus_bounds(&probe, FocusTarget::NavigationToggle)
            .expect("the navigation toolbar is rendered");
        let settings = focus_bounds(&probe, FocusTarget::SettingsTrigger)
            .expect("toolbar actions are rendered");
        let toast_text = probe
            .text
            .iter()
            .find(|(text, _)| text == "Toast placement")
            .map(|(_, bounds)| *bounds)
            .expect("the shell notice text is rendered");
        let toast_dismiss = focus_bounds(&probe, FocusTarget::ToastDismiss)
            .expect("the toast dismiss action is rendered");
        let navigation_bottom = [page_tab, navigation_toggle, settings]
            .into_iter()
            .map(|bounds| bounds.y + bounds.height)
            .fold(0.0_f32, f32::max);

        assert!(
            toast_text.y >= navigation_bottom,
            "the toast must begin below navigation in the {}x{} frame: toast={toast_text:?}, navigation_bottom={navigation_bottom}",
            viewport.width,
            viewport.height
        );
        for bounds in [toast_text, toast_dismiss] {
            assert!(
                bounds.x >= 0.0
                    && bounds.y >= 0.0
                    && bounds.x + bounds.width <= viewport.width
                    && bounds.y + bounds.height <= viewport.height,
                "toast content must stay inside the {}x{} frame: {bounds:?}",
                viewport.width,
                viewport.height
            );
        }
        if viewport.width == 720.0 {
            for target in [
                FocusTarget::NavigationToggle,
                FocusTarget::SettingsTrigger,
                FocusTarget::ContainersTrigger,
                FocusTarget::HealthTrigger,
                FocusTarget::WindowCapture,
                FocusTarget::Export,
                FocusTarget::AboutTrigger,
            ] {
                let bounds =
                    focus_bounds(&probe, target).expect("every compact toolbar action is mounted");
                assert!(
                    bounds.x >= 0.0
                        && bounds.y >= 0.0
                        && bounds.x + bounds.width <= viewport.width
                        && bounds.y + bounds.height <= viewport.height,
                    "compact toolbar action {target:?} must stay inside the {}x{} frame: {bounds:?}",
                    viewport.width,
                    viewport.height
                );
            }
        }
    }
}

#[test]
fn a_compact_toast_reserves_its_slot_around_page_controls() {
    let mut app = crate::IcedApp::demo();
    app.shell.set_feedback_activity("");
    let _ = app
        .shell
        .apply_action(AppAction::SelectPage(AppPage::Applications));
    app.shell.report_notice(
        FeedbackSource::Interaction,
        FeedbackSeverity::Success,
        FeedbackLifecycle::UntilReplaced,
        "Toast placement",
    );
    let _ = app.update(Message::WindowResized(Size::new(720.0, 480.0)));

    let mut element = crate::ui::view_root(&app);
    let probe = probe(&mut element, Size::new(720.0, 480.0));
    let toast_text = probe
        .text
        .iter()
        .find(|(text, _)| text == "Toast placement")
        .map(|(_, bounds)| *bounds)
        .expect("the shell notice text is rendered");
    let toast_dismiss =
        focus_bounds(&probe, FocusTarget::ToastDismiss).expect("the dismiss action is rendered");
    let toast_bottom = [toast_text, toast_dismiss]
        .into_iter()
        .map(|bounds| bounds.y + bounds.height)
        .fold(0.0_f32, f32::max);

    let search = focus_bounds(&probe, FocusTarget::SearchTrigger)
        .expect("the Applications search action is rendered");
    let actions = focus_bounds(&probe, FocusTarget::ProcessActionsTrigger)
        .expect("the Applications overflow action is rendered");
    for bounds in [search, actions] {
        assert!(
            bounds.y >= toast_bottom || bounds.y + bounds.height <= toast_text.y,
            "page controls must not overlap the reserved toast slot: toast={toast_text:?}, toast_bottom={toast_bottom:?}, control={bounds:?}"
        );
        assert!(
            bounds.x >= 0.0
                && bounds.y >= 0.0
                && bounds.x + bounds.width <= 720.0
                && bounds.y + bounds.height <= 480.0,
            "page controls must stay inside the compact viewport: {bounds:?}"
        );
        assert!(
            bounds.width > 0.0 && bounds.height > 0.0,
            "the visible page control must remain measured, not be dropped by the toast slot: {bounds:?}"
        );
    }
    assert!(
        toast_text.x >= 0.0
            && toast_text.y >= 0.0
            && toast_text.x + toast_text.width <= 720.0
            && toast_text.y + toast_text.height <= 480.0,
        "toast text must stay inside the compact viewport: {toast_text:?}"
    );
    assert!(
        toast_dismiss.x >= 0.0
            && toast_dismiss.y >= 0.0
            && toast_dismiss.x + toast_dismiss.width <= 720.0
            && toast_dismiss.y + toast_dismiss.height <= 480.0,
        "toast dismissal must stay inside the compact viewport: {toast_dismiss:?}"
    );
}

fn focus_bounds(probe: &CardProbe, target: FocusTarget) -> Option<Rectangle> {
    let target_id = Id::from(focus_id(target));
    probe
        .focus
        .iter()
        .find(|(id, _)| *id == target_id)
        .map(|(_, bounds)| *bounds)
}
