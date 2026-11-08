//! Vi-style key-sequence behavior through the production `handle_key` router.

use super::super::*;
use crate::runtime::keys::vim::{VimContext, VimPending};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::SearchSlot;

fn press_char(app: &mut crate::TuiApp, character: char) -> Option<PlatformEffect> {
    handle_key(
        app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(character),
            KeyModifiers::NONE,
        ),
    )
}

fn press_shift_char(app: &mut crate::TuiApp, character: char) -> Option<PlatformEffect> {
    handle_key(
        app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(character),
            KeyModifiers::SHIFT,
        ),
    )
}

fn services_app() -> crate::TuiApp {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    app
}

#[test]
fn services_gg_and_capital_g_jump_to_table_bounds() {
    let mut app = services_app();
    let rows = app.visible_services().len();
    assert!(rows > 1, "the fixture needs a bounded table jump");

    app.selected = rows - 1;
    let _ = press_char(&mut app, 'g');
    assert_eq!(
        app.vim_pending,
        VimPending::FirstRow {
            context: VimContext {
                page: AppPage::Services,
                perf_device: app.perf_device,
            }
        }
    );
    let _ = press_char(&mut app, 'g');
    assert_eq!(app.selected, 0);
    assert_eq!(app.vim_pending, VimPending::Idle);

    let _ = press_shift_char(&mut app, 'G');
    assert_eq!(app.selected, rows - 1);
    assert_eq!(app.vim_pending, VimPending::Idle);

    let _ = press_char(&mut app, 'g');
    let _ = press_char(&mut app, 'g');
    assert_eq!(app.selected, 0);
    let _ = press_shift_char(&mut app, 'G');
    assert_eq!(app.selected, rows - 1);
}

#[test]
fn services_slash_opens_the_page_slot_and_a_slash_in_search_stays_text() {
    let mut app = services_app();
    let _ = press_char(&mut app, '/');
    assert!(app.search_active());
    assert_eq!(app.shell.search_slot(), SearchSlot::Services);

    let _ = press_char(&mut app, '/');
    assert_eq!(app.shell.services_query, "/");
    assert_eq!(app.vim_pending, VimPending::Idle);
}

#[test]
fn applications_letters_stay_prefix_jumps_while_slash_opens_search() {
    let mut app = crate::demo_app();
    let _ = press_char(&mut app, 'g');
    assert_eq!(app.vim_pending, VimPending::Idle);
    assert!(!app.search_active());

    let _ = press_char(&mut app, 'd');
    assert_eq!(app.vim_pending, VimPending::Idle);
    assert!(!app.search_active());

    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    let _ = press_char(&mut app, '/');
    assert!(app.search_active());
    assert_eq!(app.shell.search_slot(), SearchSlot::Services);
}

#[test]
fn dd_clears_only_batch_marks_and_leaves_no_stale_sequence() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = press_char(&mut app, 'm');
    assert_eq!(app.shell.selected_identities().len(), 1);

    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    assert_eq!(app.shell.selected_identities().len(), 1);
    let _ = press_char(&mut app, 'd');
    let _ = press_char(&mut app, 'd');
    assert!(app.shell.selected_identities().is_empty());
    assert_eq!(app.vim_pending, VimPending::Idle);

    let _ = press_char(&mut app, 'd');
    let _ = press_char(&mut app, 'd');
    assert!(app.shell.selected_identities().is_empty());
}

#[test]
fn existing_d_bindings_take_precedence_over_the_vim_chord() {
    let mut app = services_app();
    let _ = press_char(&mut app, 'd');
    assert_eq!(app.vim_pending, VimPending::Idle);
    assert_eq!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ServiceDependencies),
        "the Services `d` binding must remain the dependency action"
    );

    let _ = press_char(&mut app, 'd');
    assert_eq!(app.vim_pending, VimPending::Idle);
}
