use super::*;
use gpui::AppContext;
use taskmanager_application::SmbiosMemoryRequestFailure;
use taskmanager_application::{SmbiosMemorySession, SmbiosMemoryState};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;
use taskmanager_ui::primitives::selectable_text;

/// A runtime without a platform client resolves the click into the honest
/// typed failure (not a hang), proving the affordance submits exactly one
/// request through the session.
#[gpui::test]
async fn authorize_affordance_submits_one_request(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        let attempt = view.shell.begin_smbios_memory_request();
        view.shell
            .reject_smbios_memory_request(attempt, FailureKind::RequiresEscalation);
        view.authorize_memory_inventory(cx);
        match view.shell.smbios_memory_state() {
            SmbiosMemoryState::Failed(failed) => assert_eq!(
                failed.failure,
                SmbiosMemoryRequestFailure::Submission(FailureKind::TemporarilyUnavailable),
                "the click must submit; the absent runtime rejects honestly"
            ),
            other => panic!("authorize must leave a terminal state, got {other:?}"),
        }
    })
    .unwrap();
}

/// The handler is gated on the authorize projection: a click while a request
/// is already in flight must not submit a second one.
#[gpui::test]
async fn authorize_affordance_is_gated_on_the_projection(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        let _ = view.shell.begin_smbios_memory_request();
        view.authorize_memory_inventory(cx);
        assert!(
            matches!(
                view.shell.smbios_memory_state(),
                SmbiosMemoryState::Loading { .. }
            ),
            "a non-authorize projection must not submit"
        );
    })
    .unwrap();
}

struct InventoryLayout {
    session: SmbiosMemorySession,
    theme: Theme,
}
impl gpui::Render for InventoryLayout {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div().w_full().h_full().child(render_memory_inventory(
            &self.theme,
            &MemoryInventoryInputs {
                state: self.session.state(),
                capability: None,
            },
            UnitPreferences::default(),
        ))
    }
}

#[gpui::test]
async fn accepted_inventory_wraps_inside_measured_card_bounds(cx: &mut gpui::TestAppContext) {
    use gpui::{VisualTestContext, px, size};
    use taskmanager_platform_contract::RequestId;
    use taskmanager_shell::fixture::smbios_memory::memory_inventory_snapshot;
    pin_english();
    cx.update(selectable_text::init);
    let win = cx.add_window(|_window, _cx| {
        let mut session = SmbiosMemorySession::default();
        let attempt = session.begin_attempt();
        assert!(session.accept_attempt(attempt, RequestId::MIN));
        assert!(session.complete(RequestId::MIN, memory_inventory_snapshot()));
        InventoryLayout {
            session,
            theme: Theme::dark(),
        }
    });
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 360.0),
        (480.0, 960.0),
    ] {
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
        let mut visual = VisualTestContext::from_window(win.into(), cx);
        let card = visual
            .debug_bounds("tm-memory-inventory-card")
            .expect("accepted card");
        assert!(card.size.height > px(0.0));
        assert!(card.right() <= px(width) && card.bottom() <= px(height));
        for selector in [
            "tm-key-value-selectable:ChannelA-DIMM0",
            "tm-key-value-selectable:ChannelB-DIMM0",
        ] {
            let value = visual
                .debug_bounds(selector)
                .expect("selectable complete inventory value");
            assert!(value.size.width > px(0.0) && value.size.height > px(0.0));
            assert!(value.left() >= card.left() && value.right() <= card.right());
            assert!(value.top() >= card.top() && value.bottom() <= card.bottom());
        }
    }
}
