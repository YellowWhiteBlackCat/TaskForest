//! Normal selectors and measured dense property bounds with product fonts.
use super::*;
use crate::app::DetailsSection;
use crate::keys::IcedKey;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Scrollable};
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Pixels, Rectangle, Size, Vector};
use taskmanager_application::{AppPage, KeyCode, Modifiers};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::ShellKeyEvent;
use taskmanager_shell::fixture::process_insights::{
    process_insights_projection, seed_process_properties_history,
};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
#[derive(Default)]
struct Bounds {
    controls: Vec<Rectangle>,
    scrolls: Vec<(Rectangle, Rectangle)>,
}
impl Operation for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        use crate::app::FocusTarget;
        let fixed = [FocusTarget::ModalClose, FocusTarget::DetailsRefresh]
            .into_iter()
            .chain(DetailsSection::ALL.into_iter().map(FocusTarget::DetailsTab))
            .chain(
                ProcessInsightFacet::ALL
                    .into_iter()
                    .map(FocusTarget::DetailsFacet),
            );
        if fixed
            .into_iter()
            .any(|target| id == Some(&Id::from(crate::focus::focus_id(target))))
        {
            self.controls.push(bounds);
        }
    }
    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        _translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        self.scrolls.push((bounds, content));
    }
}
#[test]
fn properties_use_normal_selectors_and_one_measured_body_with_fixed_actions() {
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
        let mut app = crate::IcedApp::demo();
        seed_process_properties_history(&mut app.shell);
        app.shell.application.active_page = AppPage::Applications;
        assert!(app.shell.select_row(0));
        let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
            KeyCode::Enter,
            Modifiers::NONE,
        ))));
        let target = app
            .shell
            .process_properties_target()
            .expect("normal target")
            .clone();
        let before = app
            .process_perf_history()
            .expect("provider history seeded")
            .cpu_samples();
        assert!(before.contains(&79.0));
        let before_len = before.len();
        app.sample_process_history();
        let after = app
            .process_perf_history()
            .expect("same target")
            .cpu_samples();
        assert_eq!(after.len(), before_len);
        assert!(
            after.contains(&79.0),
            "an unchanged snapshot cannot evict its oldest observed peak"
        );
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::ProcessInsights(Box::new(process_insights_projection(
                target.clone(),
            ))),
        );
        let size = Size::new(width, height);
        let _ = app.update(Message::WindowResized(size));
        app.shell.query = "hidden by filter".to_owned();
        let pss = property_rows(target.live_key().expect("key"), &app.shell);
        assert!(
            pss.iter()
                .any(|(label, value)| label == t("proc.pss") && value == "240.0 MiB")
        );
        assert!(
            pss.iter()
                .any(|(label, value)| label == t("proc.swap") && value == "64.0 MiB")
        );
        for section in DetailsSection::ALL {
            let _ = app.update(Message::SelectDetailsSection(section));
            for facet in ProcessInsightFacet::ALL {
                if section == DetailsSection::Insights {
                    let _ = app.update(Message::SelectInsightsFacet(facet));
                }
                assert_eq!(app.shell.process_properties_target(), Some(&target));
                let mut view = details_overlay(&app);
                let mut tree = Tree::new(&view);
                let node = view.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &Limits::new(Size::ZERO, size),
                );
                let mut bounds = Bounds::default();
                view.as_widget_mut()
                    .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
                assert_eq!(bounds.scrolls.len(), 1);
                let (body, _) = bounds.scrolls[0];
                assert!(
                    body.width > 0.0 && body.height >= 24.0,
                    "readable body {section:?}/{facet:?} at {size:?}: {body:?}"
                );
                assert_eq!(
                    bounds.controls.len(),
                    if section == DetailsSection::Insights {
                        13
                    } else {
                        5
                    }
                );
                for control in bounds.controls {
                    assert!(
                        control.width > 0.0
                            && control.height > 0.0
                            && control.x >= 0.0
                            && control.y >= 0.0
                            && control.x + control.width <= width + 0.5
                            && control.y + control.height < height,
                        "{section:?}/{facet:?} control {control:?} outside {size:?}"
                    );
                }
                if section != DetailsSection::Insights {
                    break;
                }
            }
        }
    }
}
