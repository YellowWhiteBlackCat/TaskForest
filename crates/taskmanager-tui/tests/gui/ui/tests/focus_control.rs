//! Intra-surface focus control: what `TuiFocusPlan` says the owning surface
//! is addressing.
//!
//! The plan's outer target names the surface that owns the keyboard; these
//! tests pin the inner control — the focused settings field, the highlighted
//! menu item, the palette row, the active properties tab, or the scrolled
//! viewport. Every drive goes through the same production helpers the key
//! handlers call, so the contract holds for real input without this suite
//! re-testing the key flow itself.

use ratatui::layout::Rect;
use taskmanager_application::{AppAction, AppPage, ConfirmationKind, SurfaceKind};

use crate::TuiApp;
use crate::ui::frame_plan::{TuiFocusControl, TuiFocusTarget, TuiFramePlan};
use crate::ui::process_properties::ProcessDetailsSection;
use crate::{TuiSurfaceKind, demo_app};

const FRAME: Rect = Rect::new(0, 0, 120, 40);

fn focused_control(app: &TuiApp) -> (TuiFocusTarget, TuiFocusControl) {
    let plan = TuiFramePlan::build(app, FRAME);
    (plan.focus.target, plan.focus.control)
}

#[test]
fn settings_plan_tracks_the_focused_field() {
    let mut app = demo_app();
    app.toggle_settings();
    app.settings_form.move_field(2);
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::Settings),
            TuiFocusControl::SettingsField(2)
        )
    );

    app.settings_form.move_field(-1);
    assert_eq!(
        focused_control(&app).1,
        TuiFocusControl::SettingsField(1),
        "the plan must mirror the form cursor, not a constant field"
    );
}

#[test]
fn service_menu_plan_tracks_the_highlighted_item() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(app.open_service_menu(), "the demo exposes a service menu");
    app.service_menu_move(1);
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::ServiceMenu),
            TuiFocusControl::MenuItem {
                surface: TuiSurfaceKind::ServiceMenu,
                index: 1,
            }
        )
    );
}

#[test]
fn process_menu_plan_tracks_the_highlighted_item() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.reconcile_applications_cursor();
    assert!(app.open_process_menu(), "the demo exposes a process menu");
    app.process_menu_move(1);
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::ProcessMenu),
            TuiFocusControl::MenuItem {
                surface: TuiSurfaceKind::ProcessMenu,
                index: 1,
            }
        )
    );
}

#[test]
fn batch_menu_plan_tracks_the_highlighted_item() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.reconcile_applications_cursor();
    let pid = app
        .selected_detail_process()
        .expect("the demo exposes a selected process")
        .pid;
    if let Some(identity) = taskmanager_shell::ProcessRowIdentity::from_parts(
        pid,
        taskmanager_test_support::fixture_start_token(pid),
    ) {
        app.shell.toggle_selected_identity(identity);
    }
    assert!(app.open_batch_menu(), "a marked row enables the batch menu");
    app.batch_menu_move(1);
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::BatchMenu),
            TuiFocusControl::MenuItem {
                surface: TuiSurfaceKind::BatchMenu,
                index: 1,
            }
        )
    );
}

#[test]
fn session_and_startup_menu_plans_track_the_highlighted_item() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    assert!(app.open_session_menu(), "the demo exposes a session menu");
    app.session_menu_move(1);
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::SessionMenu),
            TuiFocusControl::MenuItem {
                surface: TuiSurfaceKind::SessionMenu,
                index: 1,
            }
        )
    );
    app.dismiss_local_surface();

    let _ = app.apply_action(AppAction::SelectPage(AppPage::Startup));
    assert!(app.open_startup_menu(), "the demo exposes a startup menu");
    app.startup_menu_move(1);
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::StartupMenu),
            TuiFocusControl::MenuItem {
                surface: TuiSurfaceKind::StartupMenu,
                index: 1,
            }
        )
    );
}

#[test]
fn column_menu_plan_tracks_the_highlighted_item() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.toggle_column_menu();
    app.column_menu_move(1);
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::ColumnMenu),
            TuiFocusControl::MenuItem {
                surface: TuiSurfaceKind::ColumnMenu,
                index: 1,
            }
        )
    );
}

#[test]
fn palette_plan_tracks_the_highlighted_row() {
    let mut app = demo_app();
    app.open_command_palette();
    app.palette_move(3);
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::CommandPalette),
            TuiFocusControl::PaletteItem { index: 3 }
        )
    );
}

#[test]
fn properties_plan_names_the_active_tab() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.reconcile_applications_cursor();
    assert!(
        app.open_process_properties(),
        "the demo exposes a properties target"
    );
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::SharedSurface(SurfaceKind::ProcessProperties),
            TuiFocusControl::PropertiesTab(ProcessDetailsSection::Overview)
        )
    );

    app.process_properties_next_tab();
    assert_eq!(
        focused_control(&app).1,
        TuiFocusControl::PropertiesTab(ProcessDetailsSection::Performance),
        "the plan must name the tab the modal is addressing"
    );
}

#[test]
fn confirmation_plan_points_at_the_choice_controls() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(app.open_service_menu(), "the demo exposes a service menu");
    // Stop is the gated destructive action; Start needs no confirmation.
    app.service_menu_move(1);
    app.service_menu_select();
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::SharedSurface(SurfaceKind::Confirmation(
                ConfirmationKind::ServiceControl
            )),
            TuiFocusControl::ConfirmationChoice
        )
    );
}

#[test]
fn informational_surfaces_project_the_viewport_control() {
    let mut app = demo_app();

    app.toggle_about();
    assert_eq!(
        focused_control(&app),
        (
            TuiFocusTarget::LocalSurface(TuiSurfaceKind::About),
            TuiFocusControl::Viewport
        )
    );
    app.dismiss_local_surface();

    app.toggle_health();
    assert_eq!(
        focused_control(&app).1,
        TuiFocusControl::Viewport,
        "health owns a scrolled viewport, not a control list"
    );
    app.dismiss_local_surface();

    app.toggle_containers();
    assert_eq!(
        focused_control(&app).1,
        TuiFocusControl::Viewport,
        "containers owns a scrolled viewport, not a control list"
    );
    app.dismiss_local_surface();

    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    assert!(
        app.shell.open_service_log().is_some(),
        "the demo exposes a service log"
    );
    assert_eq!(
        focused_control(&app),
        (TuiFocusTarget::ServiceLog, TuiFocusControl::Viewport)
    );
}

#[test]
fn plain_content_scope_has_no_named_control() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    assert_eq!(
        focused_control(&app),
        (TuiFocusTarget::Content, TuiFocusControl::None)
    );
}

#[test]
fn suggestions_scope_projects_the_viewport_control() {
    let mut app = demo_app();
    app.toggle_suggestions();
    assert_eq!(
        focused_control(&app),
        (TuiFocusTarget::Suggestions, TuiFocusControl::Viewport,),
        "the suggestions overlay owns a scrolled viewport, not a control list"
    );
}
