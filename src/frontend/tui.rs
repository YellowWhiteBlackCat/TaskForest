//! Ratatui-only binary composition edge.

pub(super) fn run(demo: bool) {
    let result = if demo {
        taskmanager_tui::run_demo()
    } else {
        taskmanager_tui::run_live()
    };
    if let Err(error) = result {
        eprintln!("taskmanager (ui-tui): {error}");
        std::process::exit(1);
    }
}
