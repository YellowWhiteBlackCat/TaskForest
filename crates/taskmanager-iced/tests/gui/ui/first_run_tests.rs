//! test-intent: behavior

use super::*;
use taskmanager_application::first_run::FirstRunController;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_shell::fixture::setup::setup_script_info;

#[test]
fn explicit_entry_renders_a_bounded_review() {
    let mut app = crate::IcedApp::demo();
    app.first_run = FirstRunController::from_observation(Some(setup_script_info()));
    let _ = app.update(Message::FirstRun(FirstRunMessage::Open));
    drop(render_first_run(&app));
    let _ = app.update(Message::FirstRun(FirstRunMessage::RequestAction(
        SetupScriptAction::Run,
    )));
    assert!(matches!(
        app.first_run.view().phase,
        FirstRunPhase::Failed(FailureKind::TemporarilyUnavailable)
    ));
    drop(render_first_run(&app));
}

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Scrollable};
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Font, Pixels, Rectangle, Size, Vector};

#[derive(Default)]
struct Bounds {
    controls: Vec<(Option<Id>, Rectangle)>,
    scrolls: Vec<(Rectangle, Rectangle)>,
}

impl Operation for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        self.controls.push((id.cloned(), bounds));
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
fn first_run_review_keeps_native_actions_fixed_around_bounded_metadata() {
    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        Font::DEFAULT,
        Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer");
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 400.0),
        (720.0, 960.0),
    ] {
        let size = Size::new(width, height);
        let mut app = crate::IcedApp::demo();
        let _ = app.update(Message::WindowResized(size));
        let mut info = setup_script_info();
        info.run_command = info.run_command.repeat(12);
        info.revert_command = info.revert_command.repeat(12);
        app.first_run = FirstRunController::from_observation(Some(info));
        let _ = app.update(Message::FirstRun(FirstRunMessage::Open));
        let mut view = render_first_run(&app);
        let mut tree = Tree::new(&view);
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
        let mut bounds = Bounds::default();
        view.as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
        assert_eq!(
            bounds.controls.len(),
            8,
            "three metadata copies, four explicit actions and the shared Close control"
        );
        let (viewport, content) = bounds.scrolls[0];
        let fixed_actions: Vec<_> = bounds
            .controls
            .into_iter()
            .filter(|(id, _)| {
                id.as_ref().is_some_and(|id| {
                    (0..4).any(|index| {
                        *id == Id::from(focus::focus_id(crate::app::FocusTarget::FirstRunAction(
                            index,
                        )))
                    }) || *id == Id::new(focus::MODAL_CLOSE_ID)
                })
            })
            .collect();
        assert_eq!(
            fixed_actions.len(),
            5,
            "four native actions and Close are outside the metadata viewport"
        );
        for (_, control) in fixed_actions {
            assert!(control.width > 0.0 && control.height > 0.0);
            assert!(control.x >= 0.0 && control.y >= 0.0);
            assert!(
                control.x + control.width <= width && control.y + control.height < height,
                "action bounds {control:?} exceed {size:?}"
            );
        }
        assert_eq!(bounds.scrolls.len(), 1, "only setup metadata scrolls");
        assert!(viewport.width > 0.0 && viewport.height > 0.0);
        assert!(viewport.x + viewport.width <= width && viewport.y + viewport.height < height);
        assert!(content.width <= viewport.width);
        if height <= 480.0 {
            assert!(
                content.height > viewport.height,
                "metadata must be scrollable in compact reviews"
            );
        }
    }
}
