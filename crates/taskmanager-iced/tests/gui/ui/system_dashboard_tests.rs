// test-intent: behavior
//! Headless behavior tests for the System-page dashboard segment: the
//! history-window label mapping, the honest summary fold (absence is a dash,
//! never zero), and render coverage across every window selection.

use super::*;
use taskmanager_core::core::history::HistoryWindow;
use taskmanager_shell::SystemProjectionStore;

#[test]
fn window_labels_resolve_to_localized_distinct_copy() {
    let labels: Vec<&'static str> = HistoryWindow::ALL
        .iter()
        .map(|window| history_window_label(*window))
        .collect();
    assert_eq!(labels.len(), 3);
    for label in &labels {
        assert!(!label.is_empty());
    }
    // Resolution fell through to a raw key for none of the windows.
    assert_ne!(labels[0], "perf.replay.window.1h");
    assert_ne!(labels[1], "perf.replay.window.24h");
    assert_ne!(labels[2], "perf.replay.window.7d");
    // The three windows never share one label.
    assert_ne!(labels[0], labels[1]);
    assert_ne!(labels[1], labels[2]);
    assert_ne!(labels[0], labels[2]);
}

#[test]
fn summary_fold_never_fabricates_zero_from_absence() {
    let projection = SystemProjectionStore::default();
    let model = summary_model(&projection);
    assert_eq!(model.cpu, "—", "an unobserved CPU renders the dash");
    assert_eq!(model.memory, "—", "an unobserved memory renders the dash");
    assert_eq!(model.processes, None, "no inventory means no count");
    assert_eq!(
        model.active_alerts, 0,
        "an empty live mirror is a real zero"
    );
}

#[test]
fn summary_fold_tracks_the_live_projection() {
    let app = crate::IcedApp::demo();
    let model = summary_model(app.shell.projection());
    assert_eq!(
        model.processes,
        app.shell.projection().processes.as_ref().map(Vec::len),
        "the processes card mirrors the projection's inventory count"
    );
    assert_eq!(
        model.active_alerts,
        app.shell.projection().alert_active.len(),
        "the alerts card mirrors the shell's live evaluation mirror"
    );
    // Whatever the demo snapshot observes, the CPU fold is either the honest
    // dash or a formatted percentage — never a fabricated zero from absence.
    if app.shell.projection().snapshot.is_none() {
        assert_eq!(model.cpu, "—");
    } else {
        assert!(model.cpu == "—" || model.cpu.ends_with('%'));
    }
}

#[test]
fn segment_renders_for_every_window_selection_without_panic() {
    let app = crate::IcedApp::demo();
    for window in HistoryWindow::ALL {
        let _ = render_system_dashboard(&app, window);
    }
}
