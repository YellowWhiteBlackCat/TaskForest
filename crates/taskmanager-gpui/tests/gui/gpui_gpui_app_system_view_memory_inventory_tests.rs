use super::*;
use taskmanager_application::SmbiosMemoryRequestFailure;
use taskmanager_core::core::failure::FailureKind;

/// A runtime without a platform client resolves the click into the honest
/// typed failure (not a hang), proving the affordance submits exactly one
/// request through the session.
#[gpui::test]
async fn authorize_affordance_submits_one_request(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(taskmanager_theme::Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        let attempt = view.shell.begin_smbios_memory_request();
        view.shell
            .reject_smbios_memory_request(attempt, FailureKind::RequiresEscalation);
        view.authorize_memory_inventory(cx);
        match view.shell.smbios_memory_state() {
            taskmanager_application::SmbiosMemoryState::Failed(failed) => assert_eq!(
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
    let win = cx.add_window(|_window, cx| RootView::new(taskmanager_theme::Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        let _ = view.shell.begin_smbios_memory_request();
        view.authorize_memory_inventory(cx);
        assert!(
            matches!(
                view.shell.smbios_memory_state(),
                taskmanager_application::SmbiosMemoryState::Loading { .. }
            ),
            "a non-authorize projection must not submit"
        );
    })
    .unwrap();
}
