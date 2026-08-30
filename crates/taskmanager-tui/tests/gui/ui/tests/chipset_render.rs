//! System-page chipset row render tests: the platform fact renders as its own
//! device-section row when the adapter proved one, and the row disappears
//! entirely without it (honest omission, never a dash).

use taskmanager_application::{AppAction, AppPage};

use super::frame_text;

/// The System viewport is scrollable; visit enough offsets to cover the whole
/// device section no matter where the section starts.
fn system_frame_text(app: &mut crate::TuiApp) -> String {
    let mut visited = String::new();
    for offset in 0..40 {
        app.system_scroll = offset;
        visited.push_str(&frame_text(app, 120, 36));
        visited.push('\n');
    }
    visited
}

#[test]
fn system_device_section_renders_the_chipset_row_when_proved() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));
    taskmanager_shell::fixture::edit_hardware(&mut app.shell, |hardware| {
        if let Some(info) = hardware.as_mut() {
            info.chipset = Some("Z690 Chipset".into());
        }
    });

    let visited = system_frame_text(&mut app);

    assert!(
        visited.contains("Chipset"),
        "the chipset row must render:\n{visited}"
    );
    assert!(
        visited.contains("Z690 Chipset"),
        "the chipset value must render:\n{visited}"
    );
}

#[test]
fn system_device_section_omits_the_chipset_row_without_a_fact() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::System));

    let visited = system_frame_text(&mut app);

    assert!(
        !visited.contains("Chipset"),
        "an unproved chipset must not render at all:\n{visited}"
    );
}
