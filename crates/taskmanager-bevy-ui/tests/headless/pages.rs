//! test-intent: behavior
//!
//! Page-signature reservation gate: every page in `Page::ALL` (the single
//! source — the test never re-lists pages) still assembles its scene from a
//! borrow-only `PageContext` over a fresh shell, twice from one untouched
//! context. Rendering behavior belongs to each page's own wired tests under
//! `pages/`; this file only keeps the shared signature honest — a page agent
//! changing the contract breaks it deliberately here, never silently in a
//! shared file.

use taskmanager_shell::ShellApp;
use taskmanager_theme::Theme;

use crate::app::{Page, PageContext, page_scene};
use crate::palette::{UiPalette, ui_palette};

/// A real page context over a fresh shell. The palette outlives the context
/// exactly like the mount system's does.
struct Fixture {
    shell: ShellApp,
    palette: UiPalette,
    history: crate::pages::history::HistoryProjectionResource,
    process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion,
}

impl Fixture {
    fn new() -> Self {
        Self {
            shell: ShellApp::new(),
            palette: ui_palette(&Theme::dark()),
            history: crate::pages::history::HistoryProjectionResource::default(),
            process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
        }
    }

    fn context(&self) -> PageContext<'_> {
        PageContext {
            shell: &self.shell,
            process_tree_expansion: &self.process_tree_expansion,
            palette: &self.palette,
            history: &self.history.0,
        }
    }
}

#[test]
fn every_page_assembles_a_scene_twice_from_one_untouched_context() {
    // The context is borrow-only: assembling every page twice from the same
    // shell + palette must work (scenes clone what they capture). Catches a
    // page accidentally consuming or mutating the context, and keeps the
    // one-function-per-page reservation signature green.
    let fixture = Fixture::new();
    for &page in Page::ALL {
        let _ = page_scene(page, &fixture.context());
        let _ = page_scene(page, &fixture.context());
    }
}
