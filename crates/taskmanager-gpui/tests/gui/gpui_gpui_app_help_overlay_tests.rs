use super::{binding_declaration, local_binding_rows};
use crate::gpui_app::theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};
use gpui::AppContext;
use taskmanager_ui_contract::{
    Binding, CoverageStatus, FrontendShape, coverage_report, drift_findings,
};

fn theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
}

/// The seven shared page rows must all carry a label and a real shortcut —
/// the overlay must never render an empty binding column for a page the
/// router genuinely wires.
#[test]
fn every_page_help_row_has_label_and_shortcut() {
    let pages = taskmanager_shell::page_help();
    assert_eq!(pages.len(), 7, "page_help covers all seven pages");
    for page in pages {
        assert!(!page.label.is_empty(), "page {:?} has no label", page.page);
        assert!(
            !page.shortcut.is_empty(),
            "page {:?} has no shortcut",
            page.page
        );
    }
}

/// Every shared command row must carry label, description and shortcut so
/// the modal never advertises a half-empty line.
#[test]
fn every_command_help_row_has_label_description_and_shortcut() {
    let commands = taskmanager_shell::command_help();
    assert_eq!(
        commands.len(),
        taskmanager_application::CommandId::ALL.len(),
        "one row per shared CommandId"
    );
    for help in commands {
        assert!(!help.label.is_empty(), "{:?} has no label", help.command);
        assert!(
            !help.description.is_empty(),
            "{:?} has no description",
            help.command
        );
        assert!(
            !help.shortcut.is_empty(),
            "{:?} has no shortcut",
            help.command
        );
    }
}

/// The frontend-local rows must advertise the `?` binding this frontend
/// actually toggles (shell `keys.rs` local-binding honesty contract).
#[test]
fn local_binding_rows_advertise_the_question_toggle() {
    let t = theme();
    let rows = local_binding_rows(&t);
    assert_eq!(rows.len(), 1, "the GPUI local binding is exactly F1 / ?");
    assert!(
        taskmanager_shell::shell_local_bindings()
            .iter()
            .any(|binding| binding.shortcut == "?"),
        "the shell table must still document the local ? binding"
    );
}

/// Contract gate: the GPUI declaration covers every contract command
/// with an explicit entry — no missing, duplicated, or unknown command
/// — and because GPUI wires the complete shared router, every entry is
/// bound.
#[test]
fn binding_declaration_binds_every_contract_command() {
    let declaration = binding_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Gpui);
    assert_eq!(
        declaration.entries.len(),
        taskmanager_application::CommandId::ALL.len()
    );
    let report = coverage_report(&declaration);
    assert!(drift_findings(&report).is_empty(), "{report:?}");
    for (command, status) in report {
        assert!(
            matches!(status, CoverageStatus::Bound(_)),
            "{command:?}: {status:?} — GPUI advertises the complete shared router"
        );
    }
}

/// Anti-drift between the declaration and the modal: the declaration
/// mirrors the shared command rows `help_content` renders — same
/// commands, same order, same tokens. If either layer changes without
/// the other, this alignment breaks.
#[test]
fn binding_declaration_mirrors_the_help_modal_command_rows() {
    let declaration = binding_declaration();
    let commands = taskmanager_shell::command_help();
    assert_eq!(declaration.entries.len(), commands.len());
    for (entry, help) in declaration.entries.iter().zip(commands) {
        assert_eq!(entry.command, help.command);
        assert_eq!(entry.binding, Binding::Key(help.shortcut));
    }
}

/// The modal body composes for every skin: a headless window renders the
/// full help overlay (all 8 skins × both modes) and the shared command
/// rows paint a render-geometry marker — token lookups (`card_bg` /
/// `border` / `small_radius`) never panic on any variant.
#[gpui::test]
async fn modal_renders_shared_rows_for_every_skin_and_mode(cx: &mut gpui::TestAppContext) {
    for skin in Skin::ALL {
        for mode in [LightDark::Light, LightDark::Dark] {
            let win = cx.add_window(|_window, cx| {
                crate::gpui_app::root::RootView::new(
                    Theme::build(
                        skin,
                        mode,
                        HighContrast::Off,
                        ResolvedFonts::system_for(skin),
                    ),
                    cx,
                )
            });
            win.update(cx, |view, _window, cx| {
                view.mark_telemetry_frame_ready();
                view.toggle_help();
                cx.notify();
            })
            .unwrap();
            cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
                .unwrap();
            let mut vcx = gpui::VisualTestContext::from_window(win.into(), cx);
            assert!(
                vcx.debug_bounds("tm-help-cmd:Ctrl+F").is_some(),
                "skin {skin:?} / {mode:?} must render the shared Ctrl+F row"
            );
        }
    }
}
