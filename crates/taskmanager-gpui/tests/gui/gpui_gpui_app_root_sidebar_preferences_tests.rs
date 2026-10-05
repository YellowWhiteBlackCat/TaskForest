use crate::gpui_app::root::RootView;
use gpui::{AppContext, TestAppContext};
use taskmanager_theme::Theme;

#[gpui::test]
fn root_sidebar_reorder_updates_the_persisted_projection(cx: &mut TestAppContext) {
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, cx| {
        view.move_sidebar_device(
            "disk:nvme0n1",
            "cpu",
            &["cpu".into(), "disk:nvme0n1".into(), "memory".into()],
            cx,
        );
        assert_eq!(
            view.presentation_snapshot().sidebar_order(),
            ["disk:nvme0n1", "cpu", "memory"]
        );
    });
}
