//! test-intent: behavior
use super::super::*;
#[test]
fn control_arrows_expand_the_complete_live_hierarchy_and_modal_retains_ownership() {
    use crate::process_view::ProcessRow;
    use ratatui::crossterm::event::KeyCode;
    use taskmanager_shell::fixture::process_tree::seed_shell_process_tree;
    let mut app = crate::demo_app();
    app.shell.application.active_page = AppPage::Applications;
    seed_shell_process_tree(&mut app.shell).expect("root");
    let selected = app.selected_application_row_anchor();
    assert!(selected.is_some());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL),
    );
    let rows = app.process_rows_snapshot();
    let identities: std::collections::HashSet<_> = rows
        .iter()
        .filter_map(|row| match row {
            ProcessRow::TreeNode { process, .. } => Some(process.pid),
            _ => None,
        })
        .collect();
    for pid in 90_000..=90_006 {
        assert!(identities.contains(&pid));
    }
    assert!(
        rows.iter()
            .any(|row| matches!(row,ProcessRow::TreeNode{depth,..} if *depth>=3))
    );
    assert!(rows.iter().all(|row| !matches!(
        row,
        ProcessRow::Group {
            expanded: false,
            ..
        } | ProcessRow::TreeNode {
            has_children: true,
            collapsed: true,
            ..
        }
    )));
    assert_eq!(app.selected_application_row_anchor(), selected);
    drop(rows);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL),
    );
    assert!(app.process_rows_snapshot().iter().all(|row| matches!(
        row,
        ProcessRow::Group {
            expanded: false,
            ..
        }
    )));
    app.open_local_surface(crate::TuiSurface::Settings);
    let before = app.expanded_groups.clone();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL),
    );
    assert_eq!(app.expanded_groups, before);
}
