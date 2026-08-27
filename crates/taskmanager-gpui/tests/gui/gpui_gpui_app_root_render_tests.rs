use super::{CursorRefreshState, should_schedule_cursor_refresh, ui_font_with_fallback};
use crate::gpui_app::theme::{FONT_MISANS_VF, FONT_ROBOTO_MONO, Theme};

#[test]
fn cursor_refresh_is_coalesced_until_the_next_frame() {
    assert!(should_schedule_cursor_refresh(
        true,
        CursorRefreshState::Idle
    ));
    assert!(!should_schedule_cursor_refresh(
        true,
        CursorRefreshState::Scheduled
    ));
    assert!(!should_schedule_cursor_refresh(
        false,
        CursorRefreshState::Idle
    ));
}

#[test]
fn inherited_ui_font_declares_the_bundled_cjk_fallback() {
    let mut theme = Theme::dark();
    theme.ui_font = FONT_ROBOTO_MONO;

    let font = ui_font_with_fallback(&theme);
    assert_eq!(font.family.as_ref(), FONT_ROBOTO_MONO);
    assert_eq!(
        font.fallbacks
            .as_ref()
            .expect("the UI style must carry a fallback list")
            .fallback_list(),
        [FONT_MISANS_VF, FONT_ROBOTO_MONO]
    );
}
