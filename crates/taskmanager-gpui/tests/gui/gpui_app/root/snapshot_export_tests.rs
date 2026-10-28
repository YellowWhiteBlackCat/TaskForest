//! Snapshot export entry: the frontend submits the shared multi-format
//! export request and reports the typed worker outcome.

use super::*;
use taskmanager_application::i18n::t;
use taskmanager_shell::FeedbackSeverity;
use taskmanager_theme::Theme;

#[gpui::test]
fn unavailable_export_worker_reports_the_typed_feedback(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    window
        .update(cx, |view, _window, _cx| {
            view.request_snapshot_export(SystemSnapshot::default(), &[]);
            let feedback = view.shell.feedback_notice().expect("export feedback");
            assert_eq!(feedback.severity(), FeedbackSeverity::Error);
            assert_eq!(feedback.text(), t("system.export_unavailable"));
        })
        .unwrap();
}
