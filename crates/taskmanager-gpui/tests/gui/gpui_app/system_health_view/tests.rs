use gpui::{
    AppContext, Context, IntoElement, Render, ScrollHandle, TestAppContext, Window, px, size,
};

use crate::core::SmartSelfTestKind;
use crate::gpui_app::root::responsive::{PageLayoutBudget, SystemPageBudget};
use crate::gpui_app::theme::Theme;

use super::{
    SmartSelfTestConfirmationRequest, SystemHealthCallbacks, SystemHealthCaptureFixture,
    capture_english_text, capture_fixture, render_system_health,
};

struct FixtureView {
    fixture: SystemHealthCaptureFixture,
    theme: Theme,
    layout: SystemPageBudget,
}

impl Render for FixtureView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let callbacks = SystemHealthCallbacks::new(|_request, _window, _cx| {});
        let scroll = ScrollHandle::new();
        render_system_health(crate::gpui_app::system_health_view::SystemHealthViewProps {
            theme: &self.theme,
            scroll: &scroll,
            filesystems: &self.fixture.filesystems,
            sensors: &self.fixture.sensors,
            selected_disk: Some(&self.fixture.selected_disk),
            smart_report: Some(&self.fixture.smart_report),
            layout: self.layout,
            copy: &capture_english_text,
            callbacks: &callbacks,
        })
    }
}

#[test]
fn confirmation_request_is_typed_and_contains_no_execution_plan() {
    let fixture = capture_fixture();
    let request = SmartSelfTestConfirmationRequest {
        device_id: fixture.selected_disk.device_id.clone().into(),
        device_generation: fixture.selected_disk.device_generation,
        disk_name: fixture.selected_disk.name.clone(),
        disk_label: fixture.selected_disk.model.clone(),
        kind: SmartSelfTestKind::Extended,
    };
    assert_eq!(request.disk_name, "nvme0n1");
    assert_eq!(request.kind, SmartSelfTestKind::Extended);
}

/// The isolated #12/#13 surface completes layout and paint at both design
/// viewports without invoking a collector, filesystem read, or SMART command.
#[gpui::test]
async fn capture_fixture_renders_at_reference_and_compact_sizes(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| FixtureView {
        fixture: capture_fixture(),
        theme: Theme::dark(),
        layout: SystemPageBudget::from_page_layout(PageLayoutBudget::for_viewport(size(
            px(1180.0),
            px(780.0),
        ))),
    });
    for (width, height) in [(1180.0, 780.0), (720.0, 480.0)] {
        cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        window
            .update(cx, |view, _window, cx| {
                view.layout = SystemPageBudget::from_page_layout(PageLayoutBudget::for_viewport(
                    size(px(width), px(height)),
                ));
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    }
}
