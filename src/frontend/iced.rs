//! Iced-only binary composition edge.

pub(super) fn run(demo: bool) {
    if let Err(error) = taskmanager_iced::run(demo) {
        eprintln!("taskmanager (ui-iced): {error}");
        std::process::exit(1);
    }
}
