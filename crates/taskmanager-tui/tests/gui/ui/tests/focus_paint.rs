//! Focus paint consumption: overlay renderers highlight what the committed
//! focus plan names, and fail closed when it names something else.
//!
//! These tests render one surface in isolation under a hand-built
//! `TuiFocusPlan`, so the contract is exact: the plan's control decides the
//! highlight, the surface's own cursor state does not, and a control that
//! does not name the surface paints no highlight at all. In production the
//! host always passes a freshly built plan (`render_with_plan` debug-asserts
//! the rebuild matches), so these scenarios can only arise from a plan/state
//! drift — exactly the failure mode this contract turns visible.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, AppPage};

use crate::TuiTheme;
use crate::ui::frame_plan::{TuiFocusControl, TuiFocusOrder, TuiFocusPlan, TuiFocusTarget};
use crate::ui::process_properties::ProcessDetailsSection;
use crate::{TuiSurfaceKind, demo_app};

/// Pin English and serialize against the language-flipping i18n test.
fn pinned<T>(paint: impl FnOnce() -> T) -> T {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    paint()
}

/// A plan for the given local surface addressing it with `control`. The
/// `target` stays the surface under test so only the control varies.
fn local_surface_focus(surface: TuiSurfaceKind, control: TuiFocusControl) -> TuiFocusPlan {
    TuiFocusPlan {
        target: TuiFocusTarget::LocalSurface(surface),
        order: TuiFocusOrder::None,
        control,
    }
}

fn render(
    width: u16,
    height: u16,
    paint: impl FnOnce(&mut ratatui::Frame<'_>, Rect),
) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| paint(frame, frame.area()))
        .expect("draw");
    terminal
}

fn frame_text(terminal: &Terminal<TestBackend>) -> String {
    terminal.backend().to_string()
}

/// Whether the first cell of the `needle` text carries the bold modifier, for
/// assertions on highlights that are styled without a text marker.
fn span_is_bold(terminal: &Terminal<TestBackend>, needle: &str) -> bool {
    let buffer = terminal.backend().buffer();
    let symbols: Vec<char> = buffer
        .content
        .iter()
        .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
        .collect();
    let needle: Vec<char> = needle.chars().collect();
    let Some(position) = symbols
        .windows(needle.len())
        .position(|window| window == needle)
    else {
        return false;
    };
    buffer.content[position]
        .style()
        .add_modifier
        .contains(Modifier::BOLD)
}

#[test]
fn service_menu_highlights_only_the_plan_named_item() {
    pinned(|| {
        let mut app = demo_app();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
        assert!(app.open_service_menu(), "the demo exposes a service menu");
        app.service_menu_move(1);
        let crate::TuiSurface::ServiceMenu(menu) = app.local_surface().expect("the menu is open")
        else {
            unreachable!("open_service_menu stores a ServiceMenu surface");
        };

        let first = crate::ui::service_menu::action_label(crate::ui::service_menu::MENU_ACTIONS[0]);
        let second =
            crate::ui::service_menu::action_label(crate::ui::service_menu::MENU_ACTIONS[1]);

        // The plan names item 0 while the frozen state still says 1: the
        // painted highlight follows the plan, not the surface state.
        let text = frame_text(&render(52, 13, |frame, popup| {
            crate::ui::service_menu::render_service_menu_at(
                frame,
                menu,
                TuiTheme::default(),
                local_surface_focus(
                    TuiSurfaceKind::ServiceMenu,
                    TuiFocusControl::MenuItem {
                        surface: TuiSurfaceKind::ServiceMenu,
                        index: 0,
                    },
                ),
                popup,
            );
        }));
        assert!(
            text.lines()
                .any(|line| line.contains(&format!("▸ {first}"))),
            "the plan-named item must wear the focus marker, got:\n{text}"
        );
        assert!(
            !text
                .lines()
                .any(|line| line.contains(&format!("▸ {second}"))),
            "the state-selected item must NOT wear the marker, got:\n{text}"
        );

        // A control naming a different surface paints no highlight at all.
        let text = frame_text(&render(52, 13, |frame, popup| {
            crate::ui::service_menu::render_service_menu_at(
                frame,
                menu,
                TuiTheme::default(),
                local_surface_focus(
                    TuiSurfaceKind::ServiceMenu,
                    TuiFocusControl::MenuItem {
                        surface: TuiSurfaceKind::ProcessMenu,
                        index: 1,
                    },
                ),
                popup,
            );
        }));
        assert!(
            !text.contains('▸'),
            "a foreign menu control must fail closed, got:\n{text}"
        );
    });
}

#[test]
fn settings_overlay_highlights_only_the_plan_named_field() {
    pinned(|| {
        let mut app = demo_app();
        app.toggle_settings();
        let skin = t("settings.skin");
        let ui_font = t("settings.desktop_ui_font");

        // The form cursor stays on field 0; the plan names field 3.
        let text = frame_text(&render(68, 32, |frame, popup| {
            crate::ui::settings::render_settings_overlay_at(
                frame,
                &app.settings_form,
                TuiTheme::default(),
                local_surface_focus(TuiSurfaceKind::Settings, TuiFocusControl::SettingsField(3)),
                popup,
            );
        }));
        assert_eq!(
            text.matches('▸').count(),
            1,
            "exactly one field carries the focus marker, got:\n{text}"
        );
        assert!(
            text.lines()
                .any(|line| line.contains("▸ ") && line.contains(ui_font)),
            "the plan-named field must wear the marker, got:\n{text}"
        );
        assert!(
            text.lines().any(|line| line.contains(skin))
                && !text
                    .lines()
                    .any(|line| line.contains("▸ ") && line.contains(skin)),
            "field 0 must lose its marker to the plan, got:\n{text}"
        );

        // A non-field control paints no focus marker at all.
        let text = frame_text(&render(68, 32, |frame, popup| {
            crate::ui::settings::render_settings_overlay_at(
                frame,
                &app.settings_form,
                TuiTheme::default(),
                local_surface_focus(TuiSurfaceKind::Settings, TuiFocusControl::Viewport),
                popup,
            );
        }));
        assert!(
            !text.contains('▸'),
            "a viewport control must fail closed, got:\n{text}"
        );
    });
}

#[test]
fn command_palette_highlights_only_the_plan_named_row() {
    pinned(|| {
        let mut app = demo_app();
        app.open_command_palette();
        let rows = app.filtered_palette_rows();
        assert!(rows.len() > 2, "the demo palette exposes multiple rows");

        // The palette selection stays on row 0; the plan names row 2.
        let text = frame_text(&render(72, 26, |frame, popup| {
            crate::ui::help::render_command_palette_at(
                frame,
                &app,
                TuiTheme::default(),
                local_surface_focus(
                    TuiSurfaceKind::CommandPalette,
                    TuiFocusControl::PaletteItem { index: 2 },
                ),
                popup,
            );
        }));
        assert!(
            text.lines()
                .any(|line| line.contains("› ") && line.contains(rows[2].label)),
            "the plan-named row must carry the highlight marker, got:\n{text}"
        );
        assert!(
            !text
                .lines()
                .any(|line| line.contains("› ") && line.contains(rows[0].label)),
            "the state-selected row must NOT carry the marker, got:\n{text}"
        );

        // A non-palette control paints no highlight at all.
        let text = frame_text(&render(72, 26, |frame, popup| {
            crate::ui::help::render_command_palette_at(
                frame,
                &app,
                TuiTheme::default(),
                local_surface_focus(TuiSurfaceKind::CommandPalette, TuiFocusControl::Viewport),
                popup,
            );
        }));
        assert!(
            !text.contains('›'),
            "a viewport control must fail closed, got:\n{text}"
        );
    });
}

#[test]
fn properties_modal_highlights_only_the_plan_named_tab() {
    pinned(|| {
        let mut app = demo_app();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
        app.reconcile_applications_cursor();
        assert!(
            app.open_process_properties(),
            "the demo exposes a properties target"
        );
        let target = app.process_properties().expect("properties target").clone();

        let overview = format!(" {} ", t(ProcessDetailsSection::Overview.label_key()));
        let performance = format!(" {} ", t(ProcessDetailsSection::Performance.label_key()));
        let render_under = |control| {
            render(96, 30, |frame, popup| {
                crate::ui::process_properties::render_process_properties_at(
                    frame,
                    &target,
                    &app,
                    TuiTheme::default(),
                    TuiFocusPlan {
                        target: TuiFocusTarget::SharedSurface(
                            taskmanager_application::SurfaceKind::ProcessProperties,
                        ),
                        order: TuiFocusOrder::None,
                        control,
                    },
                    popup,
                );
            })
        };

        // The frozen target stays on Overview; the plan names Performance, so
        // the tab row's bold highlight must follow the plan.
        let terminal = render_under(TuiFocusControl::PropertiesTab(
            ProcessDetailsSection::Performance,
        ));
        assert!(
            span_is_bold(&terminal, &performance),
            "the plan-named tab must render bold"
        );
        assert!(
            !span_is_bold(&terminal, &overview),
            "the state-selected tab must NOT render bold"
        );

        // A non-properties control leaves every tab unhighlighted.
        let terminal = render_under(TuiFocusControl::Viewport);
        assert!(!span_is_bold(&terminal, &overview));
        assert!(!span_is_bold(&terminal, &performance));
    });
}
